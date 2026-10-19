use super::*;

impl ClientShellState {
    /// Focuses the next blocked agent after the focused one, in agent-list
    /// order across online machines, wrapping at the end.
    pub(super) fn focus_next_blocked_agent(&mut self, outcome: &mut ClientShellInput) {
        let agents: Vec<_> = super::aggregate_navigation::aggregate_agent_rows(
            &self.endpoints,
            &self.active_endpoint_id,
            self.config.agent_panel_sort,
        )
        .into_iter()
        .filter(|row| !row.endpoint.stale())
        .map(|row| {
            (
                row.endpoint.endpoint_id.clone(),
                row.agent.pane_id.clone(),
                *row.endpoint.endpoint_id == self.active_endpoint_id && row.agent.focused,
                row.agent.agent_status == crate::api::schema::AgentStatus::Blocked,
            )
        })
        .collect();
        let start = agents
            .iter()
            .position(|(_, _, focused, _)| *focused)
            .map_or(0, |index| index + 1);
        let next = (0..agents.len())
            .map(|offset| &agents[(start + offset) % agents.len()])
            .find(|(_, _, _, blocked)| *blocked);
        let Some((endpoint_id, pane_id, _, _)) = next.cloned() else {
            return;
        };
        self.focus_or_activate(
            endpoint_id,
            ClientEndpointFocusTarget::Pane(pane_id),
            outcome,
        );
        outcome.repaint = true;
    }
}
