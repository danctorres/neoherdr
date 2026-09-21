use super::*;

impl ClientShellState {
    /// Asks the runtime which agents it can start; the picker opens with its
    /// answer, so the list always reflects the runtime host, not this client.
    pub(super) fn begin_agent_tab(&mut self, outcome: &mut ClientShellInput) {
        self.push_endpoint_method_with_kind(
            crate::api::schema::Method::AgentKinds(crate::api::schema::EmptyParams::default()),
            PendingEndpointKind::PrepareAgentTab,
            outcome,
        );
    }

    pub(super) fn insert_agent_tab_text(&mut self, text: &str) -> bool {
        let Some(ClientShellOverlay::AgentTab(picker)) = self.overlay.as_mut() else {
            return false;
        };
        if !picker.opening && picker.query.insert(text) {
            if let Some(first) = picker.filtered_indices().first().copied() {
                picker.selected = first;
            }
        }
        true
    }

    pub(super) fn route_agent_tab_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let Some(ClientShellOverlay::AgentTab(picker)) = self.overlay.as_mut() else {
            return false;
        };
        let opening = picker.opening;
        let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
        let navigation = matches!(
            code,
            KeyCode::Up | KeyCode::Down | KeyCode::Enter | KeyCode::Esc
        ) || (matches!(code, KeyCode::Char('n' | 'p'))
            && modifiers == crossterm::event::KeyModifiers::CONTROL);
        if !opening && !navigation {
            if let Some(content_changed) = picker.query.handle_key(key) {
                if content_changed {
                    if let Some(first) = picker.filtered_indices().first().copied() {
                        picker.selected = first;
                    }
                }
                outcome.repaint = true;
                return true;
            }
        }
        match code {
            KeyCode::Esc if !opening => {
                self.overlay = None;
                outcome.repaint = true;
            }
            KeyCode::Enter => self.submit_agent_tab(outcome),
            KeyCode::Up if !opening => {
                self.move_agent_tab_selection(-1);
                outcome.repaint = true;
            }
            KeyCode::Down if !opening => {
                self.move_agent_tab_selection(1);
                outcome.repaint = true;
            }
            KeyCode::Char('n' | 'p') if !opening => {
                self.move_agent_tab_selection(if code == KeyCode::Char('n') { 1 } else { -1 });
                outcome.repaint = true;
            }
            _ => {}
        }
        true
    }

    pub(super) fn move_agent_tab_selection(&mut self, delta: isize) {
        let Some(ClientShellOverlay::AgentTab(picker)) = self.overlay.as_mut() else {
            return;
        };
        let filtered = picker.filtered_indices();
        if filtered.is_empty() {
            picker.selected = 0;
            return;
        }
        let current = filtered
            .iter()
            .position(|index| *index == picker.selected)
            .unwrap_or(0);
        let next = (current as isize + delta).clamp(0, filtered.len() as isize - 1) as usize;
        picker.selected = filtered[next];
    }

    pub(super) fn submit_agent_tab(&mut self, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::AgentTab(picker)) = self.overlay.as_mut() else {
            return;
        };
        if picker.opening {
            return;
        }
        let Some(index) = picker.selected_entry_index() else {
            return;
        };
        let Some(entry) = picker.entries.get(index) else {
            return;
        };
        let kind = entry.kind.clone();
        picker.selected = index;
        picker.opening = true;
        picker.error = None;
        if !self.push_endpoint_method_with_kind(
            crate::api::schema::Method::AgentOpenTab(crate::api::schema::AgentOpenTabParams {
                kind,
            }),
            PendingEndpointKind::AgentOpenTab,
            outcome,
        ) {
            if let Some(ClientShellOverlay::AgentTab(picker)) = self.overlay.as_mut() {
                picker.opening = false;
            }
        }
        outcome.repaint = true;
    }

    /// Endpoint errors already surface as client notices; this only keeps the
    /// picker consistent with the outcome.
    pub(super) fn handle_agent_tab_endpoint_result(
        &mut self,
        kind: PendingEndpointKind,
        result: Result<crate::api::schema::ResponseResult, ClientShellEndpointError>,
    ) -> bool {
        use crate::api::schema::ResponseResult;

        match (kind, result) {
            (PendingEndpointKind::PrepareAgentTab, Ok(ResponseResult::AgentKinds { kinds })) => {
                self.overlay = Some(ClientShellOverlay::AgentTab(ClientAgentTabOverlay {
                    entries: kinds
                        .into_iter()
                        .map(|kind| ClientAgentTabEntry {
                            kind: kind.kind,
                            executable: kind.executable,
                        })
                        .collect(),
                    selected: 0,
                    query: TextEditor::default(),
                    error: None,
                    opening: false,
                }));
                true
            }
            (PendingEndpointKind::AgentOpenTab, Ok(ResponseResult::TabCreated { .. })) => {
                if matches!(self.overlay, Some(ClientShellOverlay::AgentTab(_))) {
                    self.overlay = None;
                }
                true
            }
            (PendingEndpointKind::AgentOpenTab, Err(error)) => {
                if let Some(ClientShellOverlay::AgentTab(picker)) = self.overlay.as_mut() {
                    picker.opening = false;
                    picker.error = Some(error.message);
                }
                true
            }
            (PendingEndpointKind::PrepareAgentTab, Err(_)) => true,
            (_, Ok(_)) => {
                if let Some(ClientShellOverlay::AgentTab(picker)) = self.overlay.as_mut() {
                    picker.opening = false;
                }
                self.set_endpoint_error("endpoint returned an unexpected agent tab result");
                true
            }
            (_, Err(_)) => true,
        }
    }
}
