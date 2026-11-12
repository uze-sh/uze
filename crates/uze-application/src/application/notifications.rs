//! When this machine rings for a finished agent. Only the choice: ringing
//! is the client's, which owns the terminal.

use uze_core::Result;

use uze_workspace::{notifications, notifications::Chime};

use super::services::Notifications;

impl Notifications<'_> {
    /// Which finished turns ring; [`Chime::Silent`] until the operator
    /// chooses otherwise.
    #[tracing::instrument(name = "notifications.agent_finished", skip_all, err)]
    pub fn agent_finished(&self) -> Result<Chime> {
        notifications::agent_finished(&self.0.home)
    }

    /// The choice in force, and the word `config.toml` holds for it when
    /// this build does not recognise that word.
    #[tracing::instrument(name = "notifications.agent_finished_as_written", skip_all, err)]
    pub fn agent_finished_as_written(&self) -> Result<notifications::WrittenChime> {
        notifications::agent_finished_as_written(&self.0.home)
    }

    #[tracing::instrument(name = "notifications.set_agent_finished", skip_all, fields(chime = ?chime), err)]
    pub fn set_agent_finished(&self, chime: Chime) -> Result<()> {
        notifications::set_agent_finished(&self.0.home, chime)
    }
}

#[cfg(test)]
mod tests {
    use crate::{UzeApplication, UzeHome};
    use uze_workspace::notifications::Chime;

    #[test]
    fn the_chime_chosen_is_the_one_in_force() {
        let root = uze_testkit::temp::scratch("notifications-chime");
        let app = UzeApplication::new(UzeHome::at(root.join("uze")), Vec::new());
        assert_eq!(app.notifications().agent_finished().unwrap(), Chime::Silent);
        app.notifications()
            .set_agent_finished(Chime::OutOfSight)
            .unwrap();
        assert_eq!(
            app.notifications().agent_finished().unwrap(),
            Chime::OutOfSight
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
