//! `uze market`: registering, listing and linking marketplaces.

use crate::*;
use uze_application::path::Canonical as _;

pub(crate) fn run_market(app: &UzeApplication, action: MarketAction) -> Result<()> {
    match action {
        MarketAction::Add { source } => {
            let registration = with_spinner("Adding marketplace...", || {
                app.marketplace().register(&source)
            })?;
            let kind_of_read = if registration.linked {
                "linked"
            } else {
                "mirrored"
            };
            // Said only when it is not the identity itself: a checkout
            // somewhere else, or a catalogue in a subdirectory.
            let place = registration.place();
            let read = if place == registration.identity {
                kind_of_read.to_owned()
            } else {
                format!("{kind_of_read}, reads {place}")
            };
            let kind = if registration.added {
                progress::Change::Added
            } else {
                progress::Change::Attention
            };
            let mut lines = progress::change(kind, &registration.identity, Some(&read));
            if registration.resolves_here_only {
                lines.push_str(&progress::change_detail(
                    progress::Change::Attention,
                    "no origin",
                    Some("a project declaring it resolves on this machine only"),
                ));
            }
            let outcome = if registration.added {
                "1 marketplace added"
            } else {
                "marketplace already added"
            };
            print!(
                "{}",
                progress::for_terminal(&progress::change_report("market add", &lines, outcome))
            );
        }
        MarketAction::Host {
            alias,
            base,
            remove,
            format,
        } => match (alias, base) {
            (None, _) => {
                let hosts = app.marketplace().hosts()?;
                emit(format, &hosts, |hosts| render_market_hosts(hosts));
            }
            (Some(alias), _) if remove => {
                if app.marketplace().remove_host(&alias)? {
                    progress::success(&format!(
                        "Removed host {alias}; the default is github again"
                    ));
                } else {
                    progress::success(&format!("Removed host {alias}"));
                }
            }
            (Some(alias), Some(base)) => {
                app.marketplace().define_host(&alias, &base)?;
                progress::success(&format!("Host {alias} is {}", base.trim_end_matches('/')));
            }
            (Some(alias), None) => {
                app.marketplace().set_default_host(&alias)?;
                progress::success(&format!("owner/repo now resolves against {alias}"));
            }
        },
        MarketAction::List { format } => {
            let marketplaces = app.marketplace().list()?;
            emit(format, &marketplaces, |marketplaces| {
                render_market_list(marketplaces)
            });
        }
        MarketAction::Remove { name, format } => {
            let report = app.marketplace().remove(&name)?;
            emit(format, &report, render_market_removal);
            if !report.record_removed {
                return Err(uze_application::UzeError::LifecycleBlocked(format!(
                    "marketplace `{}` could not be taken off the machine entirely; \
                     it is still registered — clear the block above and run \
                     `uze market remove {name}` again",
                    report.marketplace
                )));
            }
        }
        MarketAction::Link { name, checkout } => {
            let cloned = app.marketplace().link(&name, &checkout)?;
            let checkout = checkout.canonical().unwrap_or(checkout);
            let read = if cloned {
                format!("cloned into {}", checkout.display())
            } else {
                checkout.display().to_string()
            };
            print!(
                "{}",
                progress::change_report(
                    "market link",
                    &progress::change(progress::Change::Updated, &name, Some(&read)),
                    "linked: its plugins follow your working tree, and agents.lock pins nothing \
                     from it",
                )
            );
        }
        MarketAction::Unlink { name } => {
            let report = if app.marketplace().unlink(&name)? {
                progress::change_report(
                    "market unlink",
                    &progress::change(progress::Change::Updated, &name, Some("its source")),
                    "unlinked: read from its source again",
                )
            } else {
                progress::change_report(
                    "market unlink",
                    "",
                    &format!("nothing to unlink: {name} was not linked"),
                )
            };
            print!("{}", progress::for_terminal(&report));
        }
        MarketAction::Inspect { name, format } => {
            let detail = app.marketplace().inspect(&name)?;
            emit(format, &detail, render_market_detail);
        }
    }
    Ok(())
}
