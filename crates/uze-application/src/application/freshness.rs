//! Whether an installed plugin is the revision its source offers now, and
//! when each of the two was written.

use super::*;

impl UzeApplication {
    /// Now, in seconds since the epoch — the date an offline comparison
    /// against the binary's own snapshot is about, since that snapshot is
    /// this binary and nothing older can be meant.
    fn now_unix_seconds() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default()
    }

    /// When `package` was last written, named so a person can place it in
    /// time.
    ///
    /// One Git call against a mirror already on this disk, and only on an
    /// explicit selection — the detail view, never a listing. That is the
    /// division the machine snapshot's budget forced: "is there something
    /// newer" has to be a JSON read because every listing pays it, and
    /// "how old is this" can afford to ask.
    pub(crate) fn installed_revision(&self, package: &StoredPackage) -> Option<Revision> {
        if matches!(
            package.provenance.requested,
            uze_core::PackageSource::Embedded { .. }
        ) {
            return Some(Revision::Bundled {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            });
        }
        let marketplace = package.id.marketplace();
        let record = uze_core::state::marketplace_get(&self.home, marketplace).ok()??;
        if let Some(checkout) = record.link {
            return Some(Revision::Checkout { path: checkout });
        }
        let uze_core::ResolvedSource::Git {
            commit,
            subdirectory,
            ..
        } = &package.provenance.resolved
        else {
            return None;
        };
        let repository = marketplace_catalogue::mirror_dir(&self.home, marketplace);
        // The plugin's own directory at the revision installed: what is in
        // front of the reader, not when its marketplace last moved.
        let within = subdirectory.as_ref().map(|path| path.to_string_lossy());
        let described =
            uze_core::acquisition::mirror::describe_path(&repository, commit, within.as_deref())?;
        Some(Revision::Commit {
            short: described.short,
            age: described.age,
            subject: described.subject,
        })
    }

    /// When the plugin `marketplace` offers as `plugin` was last written —
    /// asked of a plugin that is not installed, which has no provenance of
    /// its own to read.
    pub(crate) fn offered_revision(&self, marketplace: &str, plugin: &str) -> Option<Revision> {
        let record = uze_core::state::marketplace_get(&self.home, marketplace).ok()??;
        if let Some(checkout) = record.link {
            return Some(Revision::Checkout { path: checkout });
        }
        let mirrored = marketplace_catalogue::mirrored_head(&self.home, marketplace)?;
        let catalogue = self
            .catalogue_as_it_stands(marketplace, &record.source)
            .ok()?;
        let within =
            uze_core::acquisition::marketplace::plugin_subdirectory(&catalogue.manifest, plugin)
                .ok()?;
        let repository = marketplace_catalogue::mirror_dir(&self.home, marketplace);
        let described = uze_core::acquisition::mirror::describe_path(
            &repository,
            &mirrored.commit,
            Some(&within),
        )?;
        Some(Revision::Commit {
            short: described.short,
            age: described.age,
            subject: described.subject,
        })
    }

    /// Whether `package` is the one that exists.
    ///
    /// A local read in every case. The comparison a marketplace package
    /// needs is between the commit it was installed at and the commit its
    /// declared ref points at, and both are in the mirror already on this
    /// disk — so no read path reaches the network to answer it, and the
    /// answer carries the date the mirror was last brought up to date
    /// rather than pretending to be about this instant.
    ///
    /// A package built into the binary is compared against the snapshot the
    /// binary carries, which is the offline comparison that has always
    /// worked and is the only one it has. A package installed straight from
    /// a path or a URL belongs to no catalogue and reports `Unpinned`:
    /// there is no question to answer, which is a different thing from an
    /// answer UZE does not have.
    pub(crate) fn freshness_of(&self, package: &StoredPackage) -> Freshness {
        if let PackageSource::Embedded { id } = &package.provenance.requested {
            return match bootstrap::has_update(id, &package.root) {
                Ok(true) => Freshness {
                    state: FreshnessState::Behind { commits: None },
                    established_at_unix: Some(Self::now_unix_seconds()),
                },
                Ok(false) => Freshness {
                    state: FreshnessState::UpToDate,
                    established_at_unix: Some(Self::now_unix_seconds()),
                },
                Err(_) => Freshness::not_checked(),
            };
        }

        let marketplace = package.id.marketplace();
        // No catalogue knows this package: `uze add <path|git>` installs
        // under a marketplace nothing registered, so there is no ref for it
        // to be behind.
        let Ok(Some(record)) = uze_core::state::marketplace_get(&self.home, marketplace) else {
            return Freshness::unpinned();
        };
        // A marketplace read from a checkout this machine develops has no
        // meaningful "newer": the working tree is what exists, and it
        // changes whenever its author saves.
        if let Some(checkout) = record.link {
            return Freshness {
                state: FreshnessState::Linked { checkout },
                established_at_unix: None,
            };
        }
        let uze_core::ResolvedSource::Git { commit, .. } = &package.provenance.resolved else {
            return Freshness::unpinned();
        };
        // Read from the entry the mirror wrote when it was last brought up
        // to date — a JSON read, not a subprocess. Asking Git here instead
        // was measurably wrong: one `rev-parse` per installed package put
        // the machine snapshot over its budget, and this is a read path
        // that every listing and every refresh pays.
        //
        // The distance is not computed here for the same reason. "There is
        // something newer" is what a listing needs; how far is a question
        // the detail view can afford to ask.
        let Some(mirrored) = marketplace_catalogue::mirrored_head(&self.home, marketplace) else {
            return Freshness::not_checked();
        };
        let state = if mirrored.commit == *commit {
            FreshnessState::UpToDate
        } else {
            FreshnessState::Behind { commits: None }
        };
        Freshness {
            state,
            established_at_unix: Some(mirrored.at_unix),
        }
    }
}
