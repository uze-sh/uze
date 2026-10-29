//! What the declared extensions cost the workspace that last measured
//! them, kept where `uze doctor` reads it (`extension-platform` D28).
//!
//! Runs per minute and p95 exist only in a running workspace's runner,
//! and memory only in its engines; the workspace writes what it measured
//! now and then, and a `uze doctor` in another terminal reads the last of
//! it. Generated: a file that cannot be read is no measurement, and the
//! next workspace writes it again.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uze_contract::Identity;

use super::runs::{MEMORY_TARGET, P95_TARGET};
use crate::application::services::Extensions;

/// One extension's cost, as a workspace measured it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasuredCost {
    pub identity: Identity,
    pub runs_per_minute: f64,
    /// `None` before it ran anything.
    pub p95_ms: Option<f64>,
    /// What its engine and its remembered answers hold, in bytes.
    pub memory_bytes: usize,
}

impl MeasuredCost {
    /// What is over D28's targets, as `uze doctor` says it.
    pub fn over_target(&self) -> Vec<String> {
        let mut over = Vec::new();
        if let Some(p95) = self.p95_ms
            && p95 > P95_TARGET.as_secs_f64() * 1000.0
        {
            over.push(format!(
                "answers in {p95:.0} ms at p95, over {} ms",
                P95_TARGET.as_millis()
            ));
        }
        if self.memory_bytes > MEMORY_TARGET {
            over.push(format!(
                "holds {}, over {}",
                bytes_text(self.memory_bytes),
                bytes_text(MEMORY_TARGET)
            ));
        }
        over
    }
}

/// What one workspace measured, and when.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeasuredCosts {
    /// Seconds since the epoch.
    pub measured_at: u64,
    pub extensions: Vec<MeasuredCost>,
}

impl MeasuredCosts {
    /// A measurement taken now.
    pub fn now(extensions: Vec<MeasuredCost>) -> Self {
        Self {
            measured_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            extensions,
        }
    }

    /// How long ago it was taken.
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH + Duration::from_secs(self.measured_at))
            .unwrap_or_default()
    }
}

/// `bytes` as a person reads a size: KiB under a MiB, MiB above.
pub fn bytes_text(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes < KIB * KIB {
        format!("{:.0} KiB", (bytes / KIB).max(1.0))
    } else {
        format!("{:.1} MiB", bytes / (KIB * KIB))
    }
}

impl Extensions<'_> {
    /// Keeps what the workspace measured, for `uze doctor`.
    #[tracing::instrument(name = "extensions.write_costs", skip_all, err)]
    pub fn write_costs(&self, costs: &MeasuredCosts) -> uze_core::Result<()> {
        // Plain numbers and names: nothing in it fails to serialize.
        let payload = serde_json::to_vec_pretty(costs).unwrap_or_default();
        uze_core::persistence::write_atomic_unsynced(&self.0.home.extension_costs_path(), &payload)
    }

    /// What a workspace last measured, when one did and it can be read.
    #[tracing::instrument(name = "extensions.measured_costs", skip_all)]
    pub fn measured_costs(&self) -> Option<MeasuredCosts> {
        let bytes = std::fs::read(self.0.home.extension_costs_path()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UzeApplication;

    fn cost(p95_ms: Option<f64>, memory_bytes: usize) -> MeasuredCost {
        MeasuredCost {
            identity: Identity {
                marketplace: "studio".to_owned(),
                plugin: "gsd".to_owned(),
                id: "gsd-band".to_owned(),
            },
            runs_per_minute: 6.0,
            p95_ms,
            memory_bytes,
        }
    }

    #[test]
    fn what_a_workspace_measured_is_read_back_and_nothing_is_none() {
        let scratch = uze_testkit::temp::scratch("extension-costs");
        let app = UzeApplication::new(uze_core::UzeHome::at(scratch.join("uze")), Vec::new());
        assert_eq!(app.extensions().measured_costs(), None);
        let measured = MeasuredCosts::now(vec![cost(Some(3.0), 300 * 1024)]);
        app.extensions().write_costs(&measured).unwrap();
        assert_eq!(app.extensions().measured_costs(), Some(measured));
        let _ = std::fs::remove_dir_all(scratch);
    }

    #[test]
    fn only_what_is_over_d28s_targets_is_said() {
        assert!(cost(Some(3.0), 300 * 1024).over_target().is_empty());
        assert!(cost(None, 0).over_target().is_empty());
        assert_eq!(
            cost(Some(42.0), 2 * 1024 * 1024).over_target(),
            [
                "answers in 42 ms at p95, over 20 ms",
                "holds 2.0 MiB, over 1.0 MiB"
            ]
        );
    }

    #[test]
    fn a_size_reads_as_a_person_reads_it() {
        assert_eq!(bytes_text(10), "1 KiB");
        assert_eq!(bytes_text(300 * 1024), "300 KiB");
        assert_eq!(bytes_text(3 * 1024 * 1024 / 2), "1.5 MiB");
    }
}
