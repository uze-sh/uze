//! The oldest harness version an integration's deliveries are written for,
//! held against the version detected: one rule for `doctor`, `status` and
//! the delivery error that names it.

use uze_core::integration::{HarnessDetection, IntegrationPort};

/// The version `integration` needs when the detected harness is older.
pub(crate) fn unmet(
    integration: &dyn IntegrationPort,
    detection: &HarnessDetection,
) -> Option<&'static str> {
    integration
        .minimum_version()
        .filter(|minimum| detection.present && detection.is_below(minimum))
}

/// One sentence for a detected harness older than its integration needs.
pub(crate) fn sentence(
    integration: &dyn IntegrationPort,
    detection: &HarnessDetection,
) -> Option<String> {
    let minimum = unmet(integration, detection)?;
    Some(format!(
        "{} {} is older than {minimum}, the oldest version UZE supports; update it",
        integration.display_name(),
        detection.version.as_deref().unwrap_or_default()
    ))
}
