//! What an extension asks to do to an agent (`agent-bridge`): taken by the
//! host, which shows it first, writes only through `submit_notice` behind
//! `may_submit`, and limits it. Until the bridge's writing lands, a request
//! is said and refused, never typed.

use uze_extensions::contract::AgentRequest;

use crate::ui::orchestrator::WorkspaceModel;
use crate::ui::widget::ToastKind;

impl WorkspaceModel {
    /// Takes an extension's request about an agent.
    pub(in crate::ui::orchestrator) fn ask_of_agent(&mut self, request: AgentRequest) {
        self.raise_toast(
            ToastKind::Failed,
            "not sent",
            format!(
                "{}: writing to an agent from an extension is not enabled yet",
                request.label
            ),
            None,
        );
    }
}
