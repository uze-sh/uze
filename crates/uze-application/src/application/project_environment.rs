//! A project's declared agent environment: `agents.yaml`, the `agents.lock`
//! resolving it produced, and what it takes for this machine to satisfy
//! both.

use std::{collections::BTreeSet, path::Path};
use uze_core::path::Canonical as _;

use serde::Serialize;

use uze_core::{
    Result, UzeError,
    manifest::{self, DeclaredMarketplace},
    naming::NameCollisionAuthority,
    project_lock::{self, LockedMarketplace, LockedPlugin, ProjectLock},
    project_root,
    trust::TrustAuthority,
};

use super::marketplace::MarketplaceRequest;
use super::services::Project;
use super::*;

/// The manifest's spelling of a marketplace the project declares. Both
/// files name a source the same way, so this carries the keys across and
/// adds the one thing only the manifest has: the list of plugins taken from
/// it, left empty because `declare_plugin` pushes into whatever is already
/// declared rather than replacing it.
///
/// A marketplace linked to a working copy this machine develops pins nothing
/// (`record_in_lock` leaves it out by design), so the lock has no entry to
/// carry — the declared source is then what the machine registry knows: the
/// working copy, as a local path. One that is the project or sits inside
/// it is spelled relative to the project root, which is what a `path:` is
/// read against, so every clone of the project resolves it; any other does
/// not reproduce on another machine, and `plan` says so. What it must never
/// do is panic on the declaration it is obliged to write.
fn declared_marketplace_for(
    lock: &ProjectLock,
    marketplace: &str,
    linked_source: Option<std::path::PathBuf>,
    project_root: &Path,
) -> DeclaredMarketplace {
    match lock.marketplaces.get(marketplace) {
        Some(locked) => DeclaredMarketplace {
            git: Some(locked.git.clone()),
            path: None,
            r#ref: locked.r#ref.clone(),
            subdirectory: locked.subdirectory.clone(),
            plugins: Vec::new(),
        },
        None => DeclaredMarketplace {
            git: None,
            path: linked_source.map(|source| declared_path(source, project_root)),
            r#ref: None,
            subdirectory: None,
            plugins: Vec::new(),
        },
    }
}

fn declared_path(linked_source: std::path::PathBuf, project_root: &Path) -> std::path::PathBuf {
    let linked_source = linked_source.canonical().unwrap_or(linked_source);
    match linked_source.strip_prefix(project_root) {
        Ok(inside) if inside.as_os_str().is_empty() => std::path::PathBuf::from("."),
        Ok(inside) => inside.to_path_buf(),
        Err(_) => linked_source,
    }
}

/// A `path:` a clone of the project resolves the same way: relative, and
/// never climbing out of the project it is read against.
fn stays_inside_the_project(path: &Path) -> bool {
    path.is_relative()
        && !path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
}

impl Project<'_> {
    /// Read-only: what `install` would do — every link of the chain from
    /// `agents.yaml` to the projection that has not caught up.
    #[tracing::instrument(name = "project.plan", skip_all, fields(root = %root.display()), err)]
    pub fn plan(&self, root: &Path) -> Result<ProjectEnvironmentPlan> {
        let canonical =
            project_root::resolve_project_root(root)?.ok_or_else(|| UzeError::NoProject {
                hint: "a plan is about a project; stand inside one".to_owned(),
            })?;
        // The manifest is the head of the chain, not the lock. A plan
        // founded on `agents.lock` cannot see the edit a person just made
        // to `agents.yaml`, which is the most common reason to ask for one
        // at all. Both documents are read; nothing is resolved.
        let manifest = manifest::load(&canonical)?.unwrap_or_default();
        let lock = project_lock::load_lock(&canonical)?.unwrap_or_default();
        let unresolved: Vec<String> = project_lock::stale_against(&manifest, &lock)
            .into_iter()
            .map(|stale| stale.plugin)
            .collect();
        let surplus = project_lock::surplus_against(&manifest, &lock);
        let stale_projection = self.stale_projection(&canonical);
        let missing: Vec<String> = self
            .0
            .locked_plugins_missing(&lock)
            .into_iter()
            .map(|(name, _)| name.to_owned())
            .collect();

        // A marketplace declared as a path outside the project is a
        // directory on one machine. It resolves here and nowhere else —
        // including for the person who wrote it, on their next machine.
        let unreproducible_marketplaces: Vec<String> = manifest
            .marketplaces
            .iter()
            .filter(|(_, declared)| {
                declared.git.is_none()
                    && declared
                        .path
                        .as_deref()
                        .is_some_and(|path| !stays_inside_the_project(path))
            })
            .map(|(name, _)| name.clone())
            .collect();

        let has_changes = !missing.is_empty()
            || !unresolved.is_empty()
            || !surplus.is_empty()
            || stale_projection.is_some();

        Ok(ProjectEnvironmentPlan {
            missing,
            unresolved,
            surplus,
            stale_projection,
            unreproducible_marketplaces,
            has_changes,
        })
    }

    /// Whether a region the package manager owns in `AGENTS.md` is not what
    /// it should say. The workspace's region is the workspace's to report:
    /// a package command never counts it, so a project that stopped using
    /// the workspace is never told to run anything over it.
    ///
    /// One string comparison per region, and no harness is asked anything:
    /// a region's identity is a digest of the bytes it should hold.
    fn stale_projection(&self, canonical: &Path) -> Option<StaleProjection> {
        let desired = super::context::authoring_desired(canonical);
        let agents_md = canonical.join(uze_core::project_context::AGENTS_MD_FILE_NAME);
        if super::managed_region::in_step(
            &agents_md,
            uze_core::authoring::region::owns_region,
            &desired,
        ) {
            return None;
        }
        let projected_identity = uze_core::text_region::region_identities_present(&agents_md)
            .into_iter()
            .find(|identity| uze_core::authoring::region::owns_region(identity))
            .unwrap_or_else(|| "none".to_owned());
        Some(StaleProjection {
            region: uze_core::authoring::region::REGION_PREFIX.to_owned(),
            projected_identity,
        })
    }

    /// Reproduces one locked plugin: the recorded commit, never the
    /// declared `ref:` — a lock that re-resolved `main` would install
    /// whatever was pushed since, which is what it exists to prevent.
    ///
    /// The commit is fetched from the local working copy when this machine has
    /// one registered for the same repository, and from the recorded URL
    /// otherwise. Same bytes either way — a commit is a commit — but a
    /// person who set up a local marketplace should not need the network
    /// to reinstall from it.
    fn reproduce_locked_plugin(
        &self,
        locked: &LockedMarketplace,
        marketplace: &str,
        plugin: &str,
    ) -> Result<uze_core::MaterializedPackage> {
        self.locked_request(locked, marketplace)?
            .materialize_plugin(
                plugin,
                super::marketplace::MirrorAt {
                    home: &self.0.home,
                    marketplace,
                    recent: Some(super::marketplace::RECENT),
                    fetched: &self.0.mirrors_fetched,
                    catalogues: &self.0.marketplace_catalogues,
                },
            )
    }

    /// The request that reaches `locked`'s recorded commit.
    fn locked_request(
        &self,
        locked: &LockedMarketplace,
        marketplace: &str,
    ) -> Result<MarketplaceRequest> {
        let mut repository = uze_core::acquisition::marketplace::MarketplaceRepository {
            fetch: locked.git.clone(),
            identity: locked.git.clone(),
            subpath: uze_core::acquisition::marketplace::MarketplaceSubpath::of(
                locked.subdirectory.as_deref(),
            )?,
        };
        if let Ok(Some(registered)) = uze_core::state::marketplace_get(&self.0.home, marketplace)
            && let Ok(local) = uze_core::acquisition::marketplace::repository_of(&registered.source)
            && uze_core::acquisition::forge::same_repository(&local.identity, &locked.git)
        {
            repository.fetch = local.fetch;
        }
        Ok(MarketplaceRequest {
            repository,
            reference: Some(locked.revision.clone()),
        })
    }

    /// Where the machine's revision of each locked plugin stands against
    /// the lock's, for the plugins whose bytes differ from what the lock
    /// records.
    ///
    /// Asked of the mirror's history, which is one process per plugin that
    /// differs and none for one that agrees: the bytes are compared first,
    /// because a marketplace moving does not move every plugin in it. The
    /// locked commit is fetched when the mirror lacks it — the collaborator
    /// who wrote the lock pushed it, or nobody can reproduce it. `None` for
    /// a plugin the mirror cannot place.
    fn lineage_against_lock(
        &self,
        lock: &ProjectLock,
    ) -> Vec<(
        String,
        LockedPlugin,
        Option<uze_core::acquisition::mirror::Lineage>,
    )> {
        let mut placed = Vec::new();
        for (name, locked) in &lock.plugins {
            let Ok(stored) = self
                .0
                .package_by_name(&format!("{name}@{}", locked.marketplace))
            else {
                continue;
            };
            // A linked checkout pins nothing, so there is nothing to place.
            let uze_core::acquisition::ResolvedSource::Git { commit, .. } =
                &stored.provenance.resolved
            else {
                continue;
            };
            let Some(recorded) = lock.marketplaces.get(&locked.marketplace) else {
                continue;
            };
            if *commit == recorded.revision {
                continue;
            }
            if let Some(expected) = &locked.integrity
                && uze_core::digest::tree_sha256(&stored.root).ok().as_ref() == Some(expected)
            {
                continue;
            }
            let mirror =
                super::marketplace_catalogue::mirror_dir(&self.0.home, &locked.marketplace);
            let reached = self
                .locked_request(recorded, &locked.marketplace)
                .and_then(|request| {
                    uze_core::acquisition::mirror::ensure_for(
                        &request.repository.fetch,
                        &request.repository.identity,
                        &mirror,
                        request.reference.as_deref(),
                        Some(super::marketplace::RECENT),
                    )
                });
            let lineage = reached.ok().and_then(|()| {
                uze_core::acquisition::mirror::lineage(&mirror, commit, &recorded.revision)
            });
            placed.push((name.clone(), locked.clone(), lineage));
        }
        placed
    }

    /// Whether `dir` is a project's own root — the directory a path names
    /// when it names a project, as opposed to one somewhere inside it.
    #[tracing::instrument(name = "project.is_root", skip_all, fields(dir = %dir.display()))]
    pub fn is_root(&self, dir: &Path) -> bool {
        let Ok(canonical) = dir.canonical() else {
            return false;
        };
        project_root::resolve_project_root(&canonical)
            .ok()
            .flatten()
            .is_some_and(|root| root == canonical)
    }

    /// Adds a plugin to the project lock and ensures it's in the Store.
    #[tracing::instrument(name = "project.add", skip_all, fields(plugin = %plugin, marketplace = %marketplace, root = %root.display()), err)]
    pub fn add(
        &self,
        plugin: &str,
        marketplace: &str,
        root: &Path,
        authority: &dyn TrustAuthority,
        name_authority: &dyn NameCollisionAuthority,
    ) -> Result<AddPluginReport> {
        self.0.begin_operation();
        let canonical = project_root::resolve_project_root(root)?;
        // The marketplace built into UZE is not a project's to declare:
        // its plugins are installed for every project by the machine's own
        // bootstrap, so neither `agents.yaml` nor the lock records it — an
        // entry recording something nobody declared is a line that cannot
        // be acted on. A directory that is no project takes the same road:
        // the machine half alone, and the report says nothing was declared.
        let declared =
            canonical.filter(|_| marketplace != uze_core::manifest::BUILT_IN_MARKETPLACE);
        let Some(canonical) = declared else {
            // No mutation lock taken here: the call below takes it, and it
            // is not re-entrant.
            return self.0.marketplace().install_plugin_resolving(
                &format!("{plugin}@{marketplace}"),
                authority,
                name_authority,
            );
        };

        // Taken before the lock is read, so what gets written back is not a
        // copy another command changed in the meantime.
        let _mutation = uze_core::persistence::MutationLock::acquire(&self.0.home)?;
        let mut lock = project_lock::load_lock(&canonical)?.unwrap_or_default();
        let global = uze_core::state::marketplace_get(&self.0.home, marketplace)?
            .ok_or_else(|| UzeError::UnknownMarketplace(marketplace.to_owned()))?;
        let request = MarketplaceRequest::of(&global.source)?;

        // A marketplace name means one repository. The lock naming one and
        // the machine registry another is a question only a person can
        // settle.
        if let Some(recorded) = lock.marketplaces.get(marketplace)
            && !uze_core::acquisition::forge::same_repository(
                &recorded.git,
                &request.repository.identity,
            )
        {
            return Err(UzeError::MarketplaceSourceConflict {
                marketplace: marketplace.to_owned(),
                lock_source: recorded.display(),
                global_source: request.repository.identity.clone(),
            });
        }

        if let Some(existing) = lock.plugins.get(plugin)
            && existing.marketplace != marketplace
        {
            return Err(UzeError::MarketplaceMismatch {
                plugin: plugin.to_owned(),
                expected: existing.marketplace.clone(),
                found: marketplace.to_owned(),
            });
        }

        // Acquire and ingest (reuses existing lifecycle).
        let report = self.resolve_into_lock(
            &mut lock,
            plugin,
            marketplace,
            &request,
            authority,
            name_authority,
        )?;

        // The declaration comes first and the lock second: `agents.yaml` is
        // what the project meant, and the lock is what that meant resolved
        // to. Writing the lock alone would leave the manifest — the file a
        // person reads and edits — silently out of date.
        tracing::info!(target: uze_core::acquisition::git::STEP, step = "lock");
        // A linked marketplace pins nothing, so the lock has no entry —
        // the declaration names the working copy the registry link carries.
        let linked_source = uze_core::state::marketplace_get(&self.0.home, marketplace)?.and_then(
            |record| match (record.link, &record.source) {
                (Some(link), source) => Some(
                    uze_core::acquisition::marketplace::repository_of(source)
                        .and_then(|repository| repository.subpath.directory_in(&link))
                        .unwrap_or(link),
                ),
                (None, uze_core::PackageSource::Local { path }) => Some(path.clone()),
                (None, _) => None,
            },
        );
        manifest::declare_plugin(
            &canonical,
            plugin,
            marketplace,
            &declared_marketplace_for(&lock, marketplace, linked_source, &canonical),
        )?;
        project_lock::save_lock(&canonical, &lock)?;

        Ok(AddPluginReport {
            declared: true,
            ..report
        })
    }

    /// Removes a plugin's declaration and the entry it resolved to. The
    /// Store keeps the bytes: another project may want them, and this
    /// command is about what *this* project declares.
    #[tracing::instrument(name = "project.remove", skip_all, fields(plugin = %plugin, root = %root.display()), err)]
    pub fn remove(&self, plugin: &str, root: &Path) -> Result<RemoveProjectPluginReport> {
        let canonical = project_root::resolve_project_root(root)?.ok_or_else(|| {
            UzeError::NoProjectEnvironment {
                plugin: plugin.to_owned(),
            }
        })?;
        // `name@marketplace` is how the plugin was added, so it is also how
        // it is taken out; the project itself names it by `name` alone.
        let (name, marketplace) = plugin
            .split_once('@')
            .map_or((plugin, None), |(name, marketplace)| {
                (name, Some(marketplace))
            });
        let declared_elsewhere = marketplace.is_some_and(|marketplace| {
            manifest::load(&canonical)
                .ok()
                .flatten()
                .and_then(|manifest| manifest.marketplace_of(name).map(str::to_owned))
                .is_some_and(|declared| declared != marketplace)
        });
        if declared_elsewhere {
            return Ok(RemoveProjectPluginReport::NotInLock {
                plugin: plugin.to_owned(),
            });
        }
        let undeclared = manifest::undeclare_plugin(&canonical, name)?;
        let mut lock = match project_lock::load_lock(&canonical)? {
            Some(lock) => lock,
            None if undeclared => ProjectLock::default(),
            None => return Ok(RemoveProjectPluginReport::NoLock),
        };

        if lock.plugins.remove(name).is_none() && !undeclared {
            return Ok(RemoveProjectPluginReport::NotInLock {
                plugin: plugin.to_owned(),
            });
        }

        // A lock with nothing left to reproduce is not an empty lock, it is
        // no lock: leaving `agents.lock` behind declaring nothing invites
        // the belief that resolution happened.
        if lock.plugins.is_empty() && lock.marketplaces.is_empty() {
            project_lock::remove_lock(&canonical)?;
        } else {
            project_lock::save_lock(&canonical, &lock)?;
        }

        Ok(RemoveProjectPluginReport::Removed {
            plugin: plugin.to_owned(),
        })
    }

    /// Moves this project's pins to where the refs it declares point now.
    ///
    /// The half `install` deliberately does not do. `install` reproduces
    /// what `agents.lock` records — that is what lets a clone of a project
    /// reach the bytes the project was locked at, whatever has been pushed
    /// since — so moving a pin has to be asked for. Every comparable tool
    /// splits these the same way: `cargo build`/`cargo update`,
    /// `pnpm install`/`pnpm update`, `uv sync`/`uv lock --upgrade`.
    ///
    /// With no `plugin`, every plugin the manifest declares; with one, only
    /// that one, and every other lock entry is left byte-identical.
    ///
    /// Resolution goes through the same `resolve_and_install` that `add`
    /// and `install` use, so the three cannot drift in how they resolve.
    /// The trust question is asked per plugin by the install itself, and a
    /// revision that introduces executable capability the installed one did
    /// not have is held rather than applied — with the rest proceeding.
    #[tracing::instrument(name = "project.update", skip_all, fields(root = %root.display()), err)]
    pub fn update(
        &self,
        root: &Path,
        plugin: Option<&str>,
        machine: bool,
        authority: &dyn TrustAuthority,
    ) -> Result<UpdateReport> {
        self.0.begin_operation();
        let canonical = project_root::resolve_project_root(root)?;
        // `-m` states machine scope even inside a project; and where there
        // is no project, the machine half alone is all there is. Either
        // way: every installed package re-resolved from its own source, and
        // no project file written anywhere because there is none to write
        // into.
        if machine {
            return self.update_machine(plugin, authority);
        }
        let Some(canonical) = canonical else {
            return self.update_machine(plugin, authority);
        };
        let manifest = manifest::load(&canonical)?.unwrap_or_default();
        let mut lock = project_lock::load_lock(&canonical)?.unwrap_or_default();

        let declared: Vec<(String, String)> = manifest
            .declared_plugins()
            .filter(|(name, _)| plugin.is_none_or(|wanted| wanted == *name))
            .map(|(name, marketplace)| (name.to_owned(), marketplace.to_owned()))
            .collect();
        if let Some(wanted) = plugin
            && declared.is_empty()
        {
            return Err(UzeError::PluginNotUsedByProject {
                plugin: wanted.to_owned(),
            });
        }

        let wanted: Vec<(String, MarketplaceRequest)> = declared
            .iter()
            .filter_map(|(_, marketplace)| {
                let declared = manifest.marketplaces.get(marketplace)?;
                let source = Self::declared_fetch_source(&canonical, marketplace, declared).ok()?;
                Some((marketplace.clone(), MarketplaceRequest::of(&source).ok()?))
            })
            .collect();
        super::marketplace::prefetch_mirrors(
            &self.0.home,
            &self.0.mirrors_fetched,
            &self.0.marketplace_catalogues,
            &wanted,
            None,
        );

        // No mutation lock here: `Plugins::update` takes one per plugin and
        // it is not reentrant. The lock file this writes is a project file,
        // which that lock does not govern.
        let mut outcomes = Vec::new();
        for (name, marketplace) in declared {
            let declared_marketplace =
                manifest.marketplaces.get(&marketplace).ok_or_else(|| {
                    UzeError::MarketplaceMismatch {
                        plugin: name.clone(),
                        expected: marketplace.clone(),
                        found: "not declared in agents.yaml".to_owned(),
                    }
                })?;
            let fetch_source =
                Self::declared_fetch_source(&canonical, &marketplace, declared_marketplace)?;
            let request = MarketplaceRequest::of(&fetch_source)?;
            {
                let _mutation = uze_core::persistence::MutationLock::acquire(&self.0.home)?;
                self.register_marketplace(
                    &marketplace,
                    fetch_source,
                    &request.repository.identity,
                )?;
            }

            let qualified = format!("{name}@{marketplace}");
            let before = lock
                .marketplaces
                .get(&marketplace)
                .map(|entry| entry.revision.clone());
            // The revision belongs to the marketplace, so a plugin updated
            // after another from the same one finds it already moved: its
            // own entry is what says whether it moved too.
            let entry_before = lock.plugins.get(&name).cloned();

            // Resolved from what *this project declares*, never from the
            // package's own request. A package reproduced from `agents.lock`
            // carries the locked commit *as* its request, so re-resolving
            // that can only ever return the revision already installed —
            // and when that commit lives on no remote, not even that.
            //
            // Replacing, not installing over: the Store is idempotent by
            // origin, so installing again hands back what is already held.
            // `replace_with` is the path that replaces, and it puts the
            // installed revision back if the new one cannot be delivered.
            let content_before = self.installed_revision(&qualified).content;
            let (installed_id, deliveries) = if self.0.package_by_name(&qualified).is_ok() {
                let materialized = request.materialize_plugin(
                    &name,
                    super::marketplace::MirrorAt {
                        home: &self.0.home,
                        marketplace: &marketplace,
                        recent: None,
                        fetched: &self.0.mirrors_fetched,
                        catalogues: &self.0.marketplace_catalogues,
                    },
                )?;
                match self
                    .0
                    .plugins()
                    .replace_with(&qualified, materialized, authority)
                {
                    Ok(UpdatePluginReport::Updated {
                        plugin, deliveries, ..
                    }) => (plugin.id, deliveries),
                    Ok(UpdatePluginReport::Blocked { .. }) => {
                        outcomes.push(UpdateOutcome::Held {
                            plugin: name,
                            reason: "managed state has drifted; nothing was changed".to_owned(),
                        });
                        continue;
                    }
                    Err(UzeError::TrustRequired { detail, .. }) => {
                        outcomes.push(UpdateOutcome::Held {
                            plugin: name,
                            reason: format!(
                                "the newer revision asks to execute something new ({detail}); \
                                 confirm it explicitly"
                            ),
                        });
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            } else {
                let _mutation = uze_core::persistence::MutationLock::acquire(&self.0.home)?;
                let report = self.resolve_and_install(
                    &name,
                    &marketplace,
                    &request,
                    authority,
                    &uze_core::naming::NoNameCollisionAuthority,
                )?;
                (report.plugin.id, report.deliveries)
            };

            let pinned =
                self.record_in_lock(&mut lock, &name, &marketplace, &request, &installed_id)?;
            if !pinned {
                let followed = self
                    .linked_source(&marketplace)
                    .filter(|_| self.installed_revision(&installed_id).content != content_before);
                outcomes.push(match followed {
                    Some(linked_source) => UpdateOutcome::FollowedLink {
                        plugin: name,
                        linked_source,
                        deliveries,
                    },
                    None => UpdateOutcome::Held {
                        plugin: name,
                        reason: "linked to a checkout on this machine, which pins nothing"
                            .to_owned(),
                    },
                });
                continue;
            }
            let after = lock
                .marketplaces
                .get(&marketplace)
                .map(|entry| entry.revision.clone())
                .unwrap_or_default();
            let entry_moved = lock.plugins.get(&name) != entry_before.as_ref();
            outcomes.push(
                if before.as_deref() == Some(after.as_str()) && !entry_moved {
                    UpdateOutcome::AlreadyCurrent {
                        plugin: name,
                        deliveries,
                    }
                } else {
                    UpdateOutcome::Moved {
                        plugin: name,
                        revision: after,
                        deliveries,
                    }
                },
            );
            // Saved per entry: the bytes are already in the Store, and a
            // later failure must not leave the lock denying what this
            // machine now holds.
            project_lock::save_lock(&canonical, &lock)?;
        }

        // Moving a pin changes what this project's packages contribute to
        // `AGENTS.md`, so declaring and projecting stay one command, as
        // they are for `install`.
        let report = UpdateReport {
            scope: UpdateScope::Project,
            reconciled: false,
            outcomes,
        };
        let reconciled = if report.changed_content() {
            self.0.context().reconcile(&canonical)?;
            true
        } else {
            false
        };
        Ok(UpdateReport {
            reconciled,
            ..report
        })
    }

    /// The machine half of `update`, stated explicitly by `-m` or taken
    /// because there is no project: every installed package — or the one
    /// named — re-resolved from its own source, with no project file
    /// touched and the report naming each package it moved.
    ///
    /// Best-effort like the project update: one package's failure never
    /// stops the rest, and a blocked or trust-held update leaves the
    /// installed revision untouched — `Plugins::update` inspects before it
    /// detaches. A block is an outcome in the report rather than an error,
    /// so the caller can show every package's answer before failing on it.
    fn update_machine(
        &self,
        plugin: Option<&str>,
        authority: &dyn TrustAuthority,
    ) -> Result<UpdateReport> {
        let targets: Vec<String> = match plugin {
            Some(wanted) => vec![self.0.package_by_name(wanted)?.id.as_str().to_owned()],
            None => self
                .0
                .installed_packages()
                .iter()
                .map(|package| package.id.as_str().to_owned())
                .collect(),
        };
        let mut outcomes = Vec::new();
        for id in targets {
            let before = self.installed_revision(&id);
            match self.0.plugins().update(&id, authority) {
                Ok(UpdatePluginReport::Updated { deliveries, .. }) => {
                    let after = self.installed_revision(&id);
                    let linked = self
                        .0
                        .package_by_name(&id)
                        .ok()
                        .and_then(|package| self.linked_source(package.id.marketplace()));
                    outcomes.push(match linked {
                        _ if after.content == before.content => UpdateOutcome::AlreadyCurrent {
                            plugin: id,
                            deliveries,
                        },
                        Some(linked_source) => UpdateOutcome::FollowedLink {
                            plugin: id,
                            linked_source,
                            deliveries,
                        },
                        None => UpdateOutcome::Moved {
                            plugin: id,
                            revision: after.named_against(&before),
                            deliveries,
                        },
                    });
                }
                Ok(UpdatePluginReport::Blocked { plan, .. }) => {
                    outcomes.push(UpdateOutcome::Blocked {
                        plugin: id,
                        reason: format!("managed state has drifted ({plan:?})"),
                    });
                }
                Err(UzeError::TrustRequired { detail, .. }) => {
                    outcomes.push(UpdateOutcome::Held {
                        plugin: id,
                        reason: format!(
                            "the newer revision asks to execute something new ({detail}); \
                             confirm it explicitly"
                        ),
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(UpdateReport {
            scope: UpdateScope::Machine,
            reconciled: false,
            outcomes,
        })
    }

    /// The working copy `marketplace` is linked to on this machine, if any.
    fn linked_source(&self, marketplace: &str) -> Option<std::path::PathBuf> {
        uze_core::state::marketplace_get(&self.0.home, marketplace)
            .ok()
            .flatten()
            .and_then(|record| record.link)
    }

    /// What decides whether an update changed a package is its content: a
    /// linked marketplace resolves to the same working-copy path however much
    /// its files change, and a new commit can carry the same bytes.
    fn installed_revision(&self, id: &str) -> InstalledRevision {
        let package = self.0.package_by_name(id).ok();
        InstalledRevision {
            content: package
                .as_ref()
                .and_then(|package| uze_core::digest::tree_sha256(&package.root).ok()),
            provenance: package
                .map(|package| package.provenance.resolved.display())
                .unwrap_or_default(),
        }
    }

    /// Brings the project's declared environment about, in two passes over
    /// the same lifecycle an ordinary add uses
    /// (`authorize → prepare → ingest → republish → attach`):
    ///
    /// 1. **Resolution** — every plugin `agents.yaml` declares that the
    ///    lock does not answer for is acquired from the marketplace the
    ///    manifest declares it under, and what that produced is written to
    ///    `agents.lock`. The manifest is the authority; the lock is only
    ///    what asking it produced, so a project that has declared but
    ///    never resolved is exactly the case this exists for.
    /// 2. **Reproduction** — every locked plugin not yet in the Store is
    ///    installed from the source the lock itself carries, so a machine
    ///    that cloned the repository and never ran `market add` reaches
    ///    the same environment.
    ///
    /// `authority` is honored exactly as it is in `add`:
    /// `install_materialized`'s own `authorize()` call is what enforces
    /// the trust boundary, per plugin, so this function does no trust
    /// reasoning of its own.
    ///
    /// Stops at the first plugin that fails to acquire or install and
    /// returns that error — an install is either fully applied or (for
    /// whichever plugins came before the failure) partially applied with
    /// the failure surfaced, never silently partial. Plugins already
    /// installed are left untouched; already-successful ones are not
    /// rolled back on a later failure, matching `Project::add`'s own
    /// no-transaction model (the Store has no all-or-nothing multi-package
    /// primitive to build one on).
    #[tracing::instrument(name = "project.install", skip_all, fields(root = %root.display()), err)]
    pub fn install(&self, root: &Path, authority: &dyn TrustAuthority) -> Result<InstallReport> {
        self.0.begin_operation();
        let canonical = project_root::resolve_project_root(root)?;
        // No project here: there is nothing declared to bring about, and
        // creating an `agents.yaml` would turn the directory into a project
        // by being stood in — the exact thing the resolution above refuses.
        // The answer is a report of its own, which the verb reports as
        // "nothing was declared".
        let Some(canonical) = canonical else {
            return Ok(InstallReport::NoProject);
        };
        // `install` is an explicit act of setting this project up, so it is
        // the right moment to create the file a person edits — unlike
        // opening the client, which must write nothing into a repository
        // somebody is only looking at.
        manifest::ensure_exists(&canonical)?;
        let manifest = manifest::load(&canonical)?.unwrap_or_default();
        let mut lock = project_lock::load_lock(&canonical)?.unwrap_or_default();

        let mut mutation = Some(uze_core::persistence::MutationLock::acquire(&self.0.home)?);
        let mut installed_plugins = Vec::new();
        let mut skipped: Vec<SkippedPlugin> = Vec::new();
        let mut undelivered: Vec<(String, HarnessDeliveryReport)> = Vec::new();

        // Every marketplace this install is about to read, fetched at once:
        // what the manifest declares and the lock does not answer for, and
        // what the lock records that the Store does not hold yet.
        let declared: Vec<(String, MarketplaceRequest)> =
            project_lock::stale_against(&manifest, &lock)
                .iter()
                .filter_map(|stale| {
                    let declared = manifest.marketplaces.get(&stale.marketplace)?;
                    let source =
                        Self::declared_fetch_source(&canonical, &stale.marketplace, declared)
                            .ok()?;
                    Some((
                        stale.marketplace.clone(),
                        MarketplaceRequest::of(&source).ok()?,
                    ))
                })
                .collect();
        super::marketplace::prefetch_mirrors(
            &self.0.home,
            &self.0.mirrors_fetched,
            &self.0.marketplace_catalogues,
            &declared,
            Some(super::marketplace::RECENT),
        );
        let locked: Vec<(String, MarketplaceRequest)> = self
            .0
            .locked_plugins_missing(&lock)
            .into_iter()
            .filter_map(|(_, plugin)| {
                let recorded = lock.marketplaces.get(&plugin.marketplace)?;
                Some((
                    plugin.marketplace.clone(),
                    MarketplaceRequest {
                        repository: uze_core::acquisition::marketplace::MarketplaceRepository {
                            fetch: recorded.git.clone(),
                            identity: recorded.git.clone(),
                            subpath: uze_core::acquisition::marketplace::MarketplaceSubpath::of(
                                recorded.subdirectory.as_deref(),
                            )
                            .ok()?,
                        },
                        reference: Some(recorded.revision.clone()),
                    },
                ))
            })
            .collect();
        super::marketplace::prefetch_mirrors(
            &self.0.home,
            &self.0.mirrors_fetched,
            &self.0.marketplace_catalogues,
            &locked,
            Some(super::marketplace::RECENT),
        );

        // Resolution comes first: `agents.yaml` is what the project asked
        // for and the lock is only what asking produced, so a declaration
        // the lock does not answer for is resolved now rather than
        // reported as nothing to do. A clone carrying only the manifest
        // must reach the same environment as one carrying both.
        for stale in project_lock::stale_against(&manifest, &lock) {
            let marketplace = stale.marketplace.as_str();
            let declared = manifest.marketplaces.get(marketplace).ok_or_else(|| {
                UzeError::MarketplaceMismatch {
                    plugin: stale.plugin.clone(),
                    expected: marketplace.to_owned(),
                    found: "not declared in agents.yaml".to_owned(),
                }
            })?;
            // The declaration is the authority over its own source: a lock
            // recording where the plugin used to come from is the stale
            // half, so it is overwritten rather than defended.
            //
            // A marketplace this machine cannot reach *by declaration* — a
            // path only its author has — is skipped and named, not fatal.
            // A project declaring one is not broken for everybody else who
            // clones it, and failing the whole command over it hands a
            // contributor nothing where they could have had all but one.
            let fetch_source = match Self::declared_fetch_source(&canonical, marketplace, declared)
            {
                Ok(source) => source,
                Err(UzeError::MissingPath(path)) => {
                    skipped.push(SkippedPlugin {
                        plugin: stale.plugin.clone(),
                        marketplace: marketplace.to_owned(),
                        reason: format!(
                            "its marketplace is declared at {}, which this machine does not have",
                            path.display()
                        ),
                    });
                    continue;
                }
                Err(error) => return Err(error),
            };
            let request = MarketplaceRequest::of(&fetch_source)?;
            self.register_marketplace(marketplace, fetch_source, &request.repository.identity)?;
            let report = self.resolve_into_lock(
                &mut lock,
                &stale.plugin,
                marketplace,
                &request,
                authority,
                &uze_core::naming::NoNameCollisionAuthority,
            )?;
            undelivered.extend(
                report
                    .undelivered()
                    .map(|delivery| (stale.plugin.clone(), delivery.clone())),
            );
            // Saved per entry, not once at the end: bytes are already in
            // the Store, and a later failure must not leave the lock
            // denying what this machine now holds.
            project_lock::save_lock(&canonical, &lock)?;
            installed_plugins.push(stale.plugin);
        }

        // Reproduction second: what the lock records and the Store does
        // not hold yet — the fresh machine cloning a project.
        let missing: Vec<(String, LockedPlugin)> = self
            .0
            .locked_plugins_missing(&lock)
            .into_iter()
            .map(|(name, locked)| (name.to_owned(), locked.clone()))
            .collect();
        for (name, locked) in missing {
            // A fresh machine reproducing a cloned `agents.lock` has never
            // run `market add`, so this registers the
            // locked marketplace globally (from the source the lock itself
            // carries) before ingesting from it. Idempotent for a
            // same-source re-run; a genuinely different source already
            // registered under this name surfaces as `MarketplaceConflict`
            // rather than silently mis-attributing the package.
            let marketplace = locked.marketplace.as_str();
            let recorded = lock.marketplaces.get(marketplace).ok_or_else(|| {
                UzeError::MarketplaceMismatch {
                    plugin: name.clone(),
                    expected: marketplace.to_owned(),
                    found: "not declared in lock".to_owned(),
                }
            })?;
            self.register_marketplace(
                marketplace,
                PackageSource::Git {
                    url: recorded.git.clone(),
                    reference: recorded.r#ref.clone(),
                    subdirectory: recorded.subdirectory.clone(),
                },
                &recorded.git,
            )?;
            let materialized = self.reproduce_locked_plugin(recorded, marketplace, &name)?;
            // The pin is checked before the bytes are ingested, let alone
            // delivered to a harness: a moved tag, a rewritten history or a
            // substituted remote must stop here, not be discovered later by
            // reading what an agent was told to do.
            Self::verify_integrity_of(&name, &locked, materialized.root())?;
            let report = self.0.plugins().install_materialized(
                materialized,
                marketplace,
                None,
                authority,
                &uze_core::naming::NoNameCollisionAuthority,
            )?;
            undelivered.extend(
                report
                    .undelivered()
                    .map(|delivery| (name.clone(), delivery.clone())),
            );
            installed_plugins.push(name);
        }

        // Upward, third: a plugin the machine holds at a revision older than
        // the lock's — a collaborator moved the pin and this is the pull.
        // The lock is a floor, so the machine rises to it; it is never a
        // ceiling, so one already past it stays where it is, and `status`
        // says so. One the mirror cannot place is left alone and named:
        // which of two histories is wanted is not this command's to guess.
        let placed = self.lineage_against_lock(&lock);
        let behind: Vec<(String, LockedPlugin)> = placed
            .iter()
            .filter(|(_, _, lineage)| {
                *lineage == Some(uze_core::acquisition::mirror::Lineage::Older)
            })
            .map(|(name, locked, _)| (name.clone(), locked.clone()))
            .collect();
        for (name, locked, lineage) in &placed {
            if matches!(
                lineage,
                None | Some(uze_core::acquisition::mirror::Lineage::Diverged)
            ) {
                skipped.push(SkippedPlugin {
                    plugin: name.clone(),
                    marketplace: locked.marketplace.clone(),
                    reason: "this machine holds a revision that is neither older nor newer than \
                             the lock's; `uze update` records the machine's"
                        .to_owned(),
                });
            }
        }
        if !behind.is_empty() {
            // `replace_with` takes the mutation lock itself, per plugin.
            drop(mutation.take());
        }
        for (name, locked) in behind {
            let qualified = format!("{name}@{}", locked.marketplace);
            let recorded = lock.marketplaces.get(&locked.marketplace).ok_or_else(|| {
                UzeError::MarketplaceMismatch {
                    plugin: name.clone(),
                    expected: locked.marketplace.clone(),
                    found: "not declared in lock".to_owned(),
                }
            })?;
            let materialized =
                self.reproduce_locked_plugin(recorded, &locked.marketplace, &name)?;
            Self::verify_integrity_of(&name, &locked, materialized.root())?;
            match self
                .0
                .plugins()
                .replace_with(&qualified, materialized, authority)
            {
                Ok(UpdatePluginReport::Updated { deliveries, .. }) => {
                    undelivered.extend(
                        deliveries
                            .into_iter()
                            .filter(|delivery| delivery.error().is_some())
                            .map(|delivery| (name.clone(), delivery)),
                    );
                    installed_plugins.push(name);
                }
                Ok(UpdatePluginReport::Blocked { .. }) => skipped.push(SkippedPlugin {
                    plugin: name,
                    marketplace: locked.marketplace.clone(),
                    reason: "managed state has drifted; nothing was changed".to_owned(),
                }),
                Err(UzeError::TrustRequired { detail, .. }) => skipped.push(SkippedPlugin {
                    plugin: name,
                    marketplace: locked.marketplace.clone(),
                    reason: format!(
                        "the lock's revision asks to execute something new ({detail}); \
                         confirm it explicitly"
                    ),
                }),
                Err(error) => return Err(error),
            }
        }

        // Convergence, fourth: what the manifest no longer declares.
        //
        // Nothing is asked and nothing on this machine is touched. The lock
        // is derived — regenerable from the manifest, and losing an entry
        // loses nothing a person did not just delete themselves — so making
        // it agree with what the project declared is the least destructive
        // thing this command does, not the most. What *would* be
        // destructive is taking the package off the machine or out of a
        // harness, and that is `uze remove <plugin> -m`'s to do: other
        // projects share the Store, and ADR-019 keeps project scope out of
        // it.
        let mut removed_plugins = Vec::new();
        let surplus = project_lock::surplus_against(&manifest, &lock);
        if !surplus.is_empty() {
            drop(mutation.take());
            for plugin in &surplus {
                self.remove(plugin, &canonical)?;
                removed_plugins.push(plugin.clone());
            }
        }

        // Installing changes what this project's packages contribute to
        // `AGENTS.md`, and a region the package manager owns there can fall
        // behind on its own. Leaving either to a second command is how the
        // agents came to read something the project no longer says.
        let nothing_moved = installed_plugins.is_empty() && removed_plugins.is_empty();
        let attempted = (!nothing_moved || self.stale_projection(&canonical).is_some())
            .then(|| self.0.context().reconcile(&canonical));
        // Declaring an environment and projecting it are one command, so a
        // projection that failed fails the command. Swallowed, it reported
        // `NoChanges` — "everything already agrees" — over a read-only
        // `AGENTS.md` or a bridge that could not be written, and the half of
        // the environment the agents actually read never moved. Re-running
        // `install` converges the rest again and retries this.
        let reconciled = match attempted {
            Some(outcome) => {
                outcome?;
                true
            }
            None => false,
        };

        // A skip is not "nothing to do": it is something this machine
        // could not do, and it has to be said even when everything else
        // already agreed.
        if nothing_moved && !reconciled && skipped.is_empty() {
            return Ok(InstallReport::NoChanges);
        }
        Ok(InstallReport::Installed {
            plugins: installed_plugins,
            removed: removed_plugins,
            skipped,
            reconciled,
            undelivered,
        })
    }

    /// Makes sure this machine knows the marketplace, without overruling
    /// what it already knows.
    ///
    /// Identities are compared, not sources: a local clone and the remote
    /// it came from are the same marketplace, and which of the two this
    /// machine reads is its own business. A name already pointing at a
    /// different repository is a conflict only a person can settle.
    fn register_marketplace(
        &self,
        marketplace: &str,
        source: PackageSource,
        identity: &str,
    ) -> Result<()> {
        if let Some(registered) = uze_core::state::marketplace_get(&self.0.home, marketplace)? {
            let known = uze_core::acquisition::marketplace::repository_of(&registered.source)?;
            if uze_core::acquisition::forge::same_repository(&known.identity, identity) {
                return Ok(());
            }
            return Err(UzeError::MarketplaceConflict {
                name: marketplace.to_owned(),
                existing: known.identity,
                requested: identity.to_owned(),
            });
        }
        // The repository, never the project's `ref:`. The machine follows
        // its marketplaces to their newest and a declared ref is this
        // project's pin, which the lock records: registered here, a tag the
        // first project to arrive declared became the ref every update on
        // the machine resolved, for every project after it. A ref of the
        // machine's own is `market add`'s to give.
        let source = match source {
            PackageSource::Git {
                url, subdirectory, ..
            } => PackageSource::Git {
                url,
                reference: None,
                subdirectory,
            },
            other => other,
        };
        uze_core::state::marketplace_add(&self.0.home, marketplace, source)?;
        Ok(())
    }

    /// Where a declaration says to read the marketplace from. A relative
    /// `path:` is resolved against the project root and canonicalized, so
    /// the source `market add` would have registered and the one a
    /// manifest produces are the same source — two spellings of one
    /// directory would otherwise collide as a conflict under one name.
    fn declared_fetch_source(
        root: &Path,
        marketplace: &str,
        declared: &DeclaredMarketplace,
    ) -> Result<PackageSource> {
        if let Some(url) = &declared.git {
            return Ok(PackageSource::Git {
                url: url.clone(),
                reference: declared.r#ref.clone(),
                subdirectory: declared.subdirectory.clone(),
            });
        }
        let declared_path = declared.path.as_ref().ok_or_else(|| {
            UzeError::UnknownPackage(format!("marketplace `{marketplace}` declares no source"))
        })?;
        let joined = root.join(declared_path);
        let path = joined
            .canonical()
            .map_err(|_| UzeError::MissingPath(joined.clone()))?;
        Ok(PackageSource::Local { path })
    }

    /// Acquires one plugin from `marketplace` and installs it, returning
    /// what landed.
    ///
    /// Deliberately writes nothing to the lock. `add`, `install` and
    /// `update` must not differ in *how* they resolve — that is what this
    /// being one function buys — and they legitimately differ in what they
    /// record: a marketplace linked to a working copy on this machine is
    /// resolved exactly like any other and pins nothing, which a branch
    /// inside a shared function would be the wrong way to say.
    fn resolve_and_install(
        &self,
        plugin: &str,
        marketplace: &str,
        request: &MarketplaceRequest,
        authority: &dyn TrustAuthority,
        name_authority: &dyn NameCollisionAuthority,
    ) -> Result<AddPluginReport> {
        let materialized = request.materialize_plugin(
            plugin,
            super::marketplace::MirrorAt {
                home: &self.0.home,
                marketplace,
                recent: Some(super::marketplace::RECENT),
                fetched: &self.0.mirrors_fetched,
                catalogues: &self.0.marketplace_catalogues,
            },
        )?;
        self.0.plugins().install_materialized(
            materialized,
            marketplace,
            None,
            authority,
            name_authority,
        )
    }

    /// Records in `lock` what installing `plugin` from `marketplace`
    /// produced.
    ///
    /// Read back from the Store rather than from the request — the
    /// discipline `Provenance` exists to enforce: what actually landed is
    /// the source of truth.
    fn record_in_lock(
        &self,
        lock: &mut ProjectLock,
        plugin: &str,
        marketplace: &str,
        request: &MarketplaceRequest,
        installed_id: &str,
    ) -> Result<bool> {
        let stored = self.0.package_by_name(installed_id)?;
        let reproducible = stored.provenance.resolved.lock_revision().is_some();
        // A marketplace read from a working copy this machine develops resolves
        // to a path, not a commit, and pins nothing. Writing one would put
        // a revision taken from unpublished work into a file a
        // collaborator pulls — the mistake `pnpm link` also refuses. The
        // project's existing entry is left exactly as it is.
        let uze_core::acquisition::ResolvedSource::Git { commit, .. } = &stored.provenance.resolved
        else {
            return Ok(false);
        };
        // Pinnable bytes that cannot be read fail the lock: one written
        // without the digest they were owed would reproduce anything.
        let integrity = if reproducible {
            Some(self.0.store.digest(&stored)?)
        } else {
            None
        };
        lock.plugins.insert(
            plugin.to_owned(),
            LockedPlugin::resolved(marketplace, integrity),
        );
        lock.marketplaces.insert(
            marketplace.to_owned(),
            LockedMarketplace {
                git: request.repository.identity.clone(),
                r#ref: request.reference.clone(),
                subdirectory: request.repository.subpath.as_path(),
                revision: commit.clone(),
            },
        );
        Ok(true)
    }

    fn resolve_into_lock(
        &self,
        lock: &mut ProjectLock,
        plugin: &str,
        marketplace: &str,
        request: &MarketplaceRequest,
        authority: &dyn TrustAuthority,
        name_authority: &dyn NameCollisionAuthority,
    ) -> Result<AddPluginReport> {
        let report =
            self.resolve_and_install(plugin, marketplace, request, authority, name_authority)?;
        self.record_in_lock(lock, plugin, marketplace, request, &report.plugin.id)?;
        Ok(report)
    }

    /// Refuses bytes that are not the bytes the lock pinned. An entry with
    /// no `integrity` is not checked — a local path has none to record, and
    /// so has nothing to contradict.
    fn verify_integrity_of(plugin: &str, locked: &LockedPlugin, acquired: &Path) -> Result<()> {
        let Some(expected) = &locked.integrity else {
            return Ok(());
        };
        let found = uze_core::digest::tree_sha256(acquired).map_err(UzeError::read(acquired))?;
        if &found == expected {
            return Ok(());
        }
        Err(UzeError::IntegrityMismatch {
            plugin: plugin.to_owned(),
            expected: expected.clone(),
            found,
        })
    }

    /// A summary of this project's `agents.lock` for `uze status` — never
    /// errors: a missing lock is `Absent`, and a lock that fails to parse
    /// is `Malformed` rather than failing `status` itself, since `status`
    /// is meant to diagnose exactly this kind of problem, not refuse to
    /// run because of it.
    #[tracing::instrument(name = "project.lock_status", skip_all, fields(root = %root.display()))]
    pub fn lock_status(&self, root: &Path) -> ProjectLockStatus {
        let canonical = match project_root::resolve_project_root(root) {
            Ok(Some(canonical)) => canonical,
            Ok(None) | Err(_) => return ProjectLockStatus::Absent,
        };
        let lock = match project_lock::load_lock(&canonical) {
            Ok(Some(lock)) => lock,
            Ok(None) => return ProjectLockStatus::Absent,
            Err(error) => {
                return ProjectLockStatus::Malformed {
                    reason: error.to_string(),
                };
            }
        };
        let missing: BTreeSet<&str> = self
            .0
            .locked_plugins_missing(&lock)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let plugins = lock
            .plugins
            .iter()
            .map(|(name, locked)| ProjectPluginHealth {
                plugin: name.clone(),
                installed: !missing.contains(name.as_str()),
                differs_from_lock: self.differs_from_lock(&lock, name, locked),
            })
            .collect();
        ProjectLockStatus::Present { plugins }
    }
}

impl Project<'_> {
    /// Whether the machine's bytes for `name` are not the ones `locked`
    /// records.
    ///
    /// No direction, deliberately: which is newer is a question for the
    /// mirror's history, a process per plugin this read cannot afford. The
    /// commit answers first, from the Store's own record; only a commit
    /// that differs reads the bytes, since a marketplace moving does not
    /// move every plugin in it.
    fn differs_from_lock(&self, lock: &ProjectLock, name: &str, locked: &LockedPlugin) -> bool {
        let Ok(stored) = self
            .0
            .package_by_name(&format!("{name}@{}", locked.marketplace))
        else {
            return false;
        };
        let uze_core::acquisition::ResolvedSource::Git { commit, .. } = &stored.provenance.resolved
        else {
            return false;
        };
        if lock
            .marketplaces
            .get(&locked.marketplace)
            .is_none_or(|recorded| recorded.revision == *commit)
        {
            return false;
        }
        locked.integrity.as_ref().is_some_and(|expected| {
            uze_core::digest::tree_sha256(&stored.root).ok().as_ref() != Some(expected)
        })
    }
}

/// `uze status`'s view of this project's `agents.lock` — deliberately
/// smaller than `ProjectEnvironmentPlan` (which
/// `agent context inspect`-equivalent commands already cover in full): just
/// enough to answer "is there a lock, and does it match what's installed."
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectLockStatus {
    Absent,
    Malformed { reason: String },
    Present { plugins: Vec<ProjectPluginHealth> },
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectPluginHealth {
    pub plugin: String,
    pub installed: bool,
    /// Installed, at a revision other than the lock's. The ordinary state
    /// once the machine has updated past a project: the lock is a floor for
    /// a machine that has less, not a record of what runs here.
    pub differs_from_lock: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectEnvironmentPlan {
    /// Recorded in the lock, and absent from this machine's Store.
    pub missing: Vec<String>,
    /// Declared in `agents.yaml`, and the lock does not answer for it.
    pub unresolved: Vec<String>,
    /// In the lock, and the manifest no longer declares it.
    pub surplus: Vec<String>,
    /// A region the package manager owns in `AGENTS.md` is behind.
    pub stale_projection: Option<StaleProjection>,
    /// Marketplaces this project declares that resolve nowhere but the
    /// machine that declared them. Not a fault here — they work where they
    /// are — but a project depending on one cannot be reproduced
    /// elsewhere, and silence about that is what lets somebody find out by
    /// handing a teammate a repository that does not work.
    pub unreproducible_marketplaces: Vec<String>,
    pub has_changes: bool,
}

/// A region the package manager owns in `AGENTS.md` is not what it should
/// say, so the agents reading the file are reading something else.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StaleProjection {
    /// Which region: its identity's prefix.
    pub region: String,
    /// The identity the region in `AGENTS.md` still carries, or `none`.
    pub projected_identity: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemoveProjectPluginReport {
    NoLock,
    NotInLock { plugin: String },
    Removed { plugin: String },
}

/// A package as an update compares it: by its content, named by where it
/// came from.
struct InstalledRevision {
    content: Option<String>,
    provenance: String,
}

impl InstalledRevision {
    /// The name a moved package's new revision reads under: where it came
    /// from when that moved too, else its content digest, since a source
    /// path that stayed put says nothing about what changed.
    fn named_against(self, before: &Self) -> String {
        if self.provenance != before.provenance {
            return self.provenance;
        }
        self.content
            .map(|digest| digest.trim_start_matches("sha256:").to_owned())
            .unwrap_or(self.provenance)
    }
}

/// What `uze update` did to each plugin it considered.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdateOutcome {
    /// The declared ref had moved, and the project now points at where it
    /// points.
    Moved {
        plugin: String,
        revision: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        deliveries: Vec<HarnessDeliveryReport>,
    },
    /// The declared ref resolves to the revision already locked.
    AlreadyCurrent {
        plugin: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        deliveries: Vec<HarnessDeliveryReport>,
    },
    /// The package's marketplace is linked to a working copy on this machine,
    /// and the Store took in what its working tree holds now. Nothing was
    /// pinned: a working tree is not a revision a lock can name.
    FollowedLink {
        plugin: String,
        /// The working tree the marketplace is linked to. Serialized under its
        /// earlier name, which `--format json` readers already know.
        #[serde(rename = "checkout")]
        linked_source: std::path::PathBuf,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        deliveries: Vec<HarnessDeliveryReport>,
    },
    /// Considered and deliberately not moved, with the reason — a
    /// marketplace linked to a working copy on this machine pins nothing, and a
    /// revision introducing execution waits for an explicit decision.
    Held { plugin: String, reason: String },
    /// The safety check refused to detach what is installed, so it was left
    /// exactly as it was. Unlike `Held`, this fails the command: a script
    /// running `update -m x && …` must not be told the update happened.
    Blocked { plugin: String, reason: String },
}

/// Which pins an update considered: a project's declared ones, or every
/// package this machine holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateScope {
    Project,
    Machine,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateReport {
    pub scope: UpdateScope,
    pub outcomes: Vec<UpdateOutcome>,
    /// Whether the project's context was reconciled afterwards.
    pub reconciled: bool,
}

impl UpdateReport {
    pub fn moved(&self) -> bool {
        self.outcomes
            .iter()
            .any(|outcome| matches!(outcome, UpdateOutcome::Moved { .. }))
    }

    /// Whether any package's content changed, pinned or not — which is what
    /// the project's context has to follow.
    pub fn changed_content(&self) -> bool {
        self.moved()
            || self
                .outcomes
                .iter()
                .any(|outcome| matches!(outcome, UpdateOutcome::FollowedLink { .. }))
    }

    /// Every harness an updated package stayed installed without reaching,
    /// with the package it is about.
    pub fn undelivered(&self) -> impl Iterator<Item = (&str, &HarnessDeliveryReport)> {
        self.outcomes.iter().flat_map(|outcome| {
            let (plugin, deliveries) = match outcome {
                UpdateOutcome::Moved {
                    plugin, deliveries, ..
                }
                | UpdateOutcome::FollowedLink {
                    plugin, deliveries, ..
                }
                | UpdateOutcome::AlreadyCurrent { plugin, deliveries } => {
                    (plugin.as_str(), deliveries.as_slice())
                }
                _ => ("", &[][..]),
            };
            deliveries
                .iter()
                .filter(|delivery| delivery.error().is_some())
                .map(move |delivery| (plugin, delivery))
        })
    }

    /// The packages the safety check refused to touch.
    pub fn blocked(&self) -> impl Iterator<Item = &str> {
        self.outcomes.iter().filter_map(|outcome| match outcome {
            UpdateOutcome::Blocked { plugin, .. } => Some(plugin.as_str()),
            _ => None,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstallReport {
    /// The declared environment, the lock and the machine already agreed,
    /// and the projection was current; nothing to do.
    NoChanges,
    /// There is no project here, so nothing was declared to bring about.
    NoProject,
    /// The project's environment moved: plugins resolved or reproduced,
    /// plugins the manifest no longer declares removed, and the project
    /// context left reconciled.
    Installed {
        plugins: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        removed: Vec<String>,
        /// Plugins whose marketplace this machine cannot reach. Named
        /// rather than silently absent: a half-installed environment that
        /// says nothing looks complete.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<SkippedPlugin>,
        #[serde(default)]
        reconciled: bool,
        /// Harnesses a plugin installed here could not be delivered to.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        undelivered: Vec<(String, HarnessDeliveryReport)>,
    },
}

impl InstallReport {
    /// Each harness a plugin this install placed could not reach.
    pub fn undelivered(&self) -> impl Iterator<Item = (&str, &HarnessDeliveryReport)> {
        let entries = match self {
            Self::Installed { undelivered, .. } => undelivered.as_slice(),
            _ => &[],
        };
        entries
            .iter()
            .map(|(plugin, delivery)| (plugin.as_str(), delivery))
    }
}

/// One plugin `install` could not reach, and why.
#[derive(Clone, Debug, Serialize)]
pub struct SkippedPlugin {
    pub plugin: String,
    pub marketplace: String,
    pub reason: String,
}

/// Read by the project environment, `uze status` and the workspace
/// overview, so it belongs to the type that owns the state rather than to
/// any one view.
impl UzeApplication {
    /// The plugins `lock` records that this machine's Store does not hold,
    /// with their names — `LockedPlugin` itself carries none (it is the
    /// map's key).
    pub(crate) fn locked_plugins_missing<'lock>(
        &self,
        lock: &'lock ProjectLock,
    ) -> Vec<(&'lock str, &'lock LockedPlugin)> {
        let installed: BTreeSet<String> = self
            .installed_packages()
            .into_iter()
            .map(|package| package.id.as_str().to_owned())
            .collect();
        lock.plugins
            .iter()
            .filter(|(name, locked)| !installed.contains(&format!("{name}@{}", locked.marketplace)))
            .map(|(name, locked)| (name.as_str(), locked))
            .collect()
    }
}
