//! Which plugin actions the active endpoint can run, so default menu entries
//! for bundled plugins (such as worktrunk in the git menu) hide once the
//! plugin is removed or disabled instead of failing when pressed.

use super::*;

impl ClientShellState {
    /// Refreshes plugin availability when `group` has default plugin-action
    /// members. The hint re-renders when the answer arrives.
    pub(super) fn refresh_plugin_actions_for_group(
        &mut self,
        group: &str,
        outcome: &mut ClientShellInput,
    ) {
        let has_plugin_members = self
            .config
            .keybinds
            .keybinds
            .groups
            .iter()
            .filter(|candidate| candidate.id == group)
            .flat_map(|group| &group.members)
            .any(|member| {
                matches!(
                    member.action,
                    crate::config::KeyGroupAction::PluginAction(_)
                )
            });
        if has_plugin_members {
            self.refresh_plugin_actions(outcome);
        }
    }

    /// Asks the active endpoint for its plugins. Sent when a menu with plugin
    /// members opens, at human input frequency, so it reflects plugin changes
    /// made from the CLI while the client runs; the help panel reuses the last
    /// answer. Failures are silent: the members just stay hidden.
    pub(super) fn refresh_plugin_actions(&mut self, outcome: &mut ClientShellInput) {
        let method =
            crate::api::schema::Method::PluginList(crate::api::schema::PluginListParams::default());
        if !self.endpoint_is_online(&self.active_endpoint_id)
            || !self.supports_endpoint_method(&method)
        {
            return;
        }
        self.push_endpoint_method_with_kind(method, PendingEndpointKind::PluginActions, outcome);
    }

    pub(super) fn complete_plugin_actions(
        &mut self,
        result: Result<crate::api::schema::ResponseResult, ClientShellEndpointError>,
    ) -> (bool, Vec<ClientShellAction>) {
        let Ok(crate::api::schema::ResponseResult::PluginList { plugins }) = result else {
            return (false, Vec::new());
        };
        let actions = available_plugin_actions(&plugins);
        if self.plugin_actions.as_ref() == Some(&actions) {
            return (false, Vec::new());
        }
        self.plugin_actions = Some(actions);
        let Some(group) = self.active_key_group.clone() else {
            return (false, Vec::new());
        };
        self.maybe_show_key_group_hint(&group);
        (true, Vec::new())
    }

    /// Whether a menu member can run on the active endpoint. Unavailable
    /// members neither show in which-key or help nor dispatch.
    pub(super) fn key_group_action_available(
        &self,
        action: &crate::config::KeyGroupAction,
    ) -> bool {
        match action {
            crate::config::KeyGroupAction::Builtin(action) => {
                self.builtin_action_available(*action)
            }
            crate::config::KeyGroupAction::PluginAction(action) => {
                self.plugin_action_available(action)
            }
            crate::config::KeyGroupAction::Command(_) => true,
        }
    }

    pub(super) fn plugin_action_available(&self, qualified_id: &str) -> bool {
        self.plugin_actions
            .as_ref()
            .is_some_and(|actions| actions.contains(qualified_id))
    }
}

/// Qualified ids (`plugin.action`) of every action an enabled plugin with a
/// readable manifest declares.
fn available_plugin_actions(
    plugins: &[crate::api::schema::InstalledPluginInfo],
) -> HashSet<String> {
    plugins
        .iter()
        .filter(|plugin| {
            plugin.enabled
                && !plugin.warnings.iter().any(|warning| {
                    warning.starts_with(
                        crate::persist::plugin_registry::MANIFEST_UNAVAILABLE_WARNING_PREFIX,
                    )
                })
        })
        .flat_map(|plugin| {
            plugin
                .actions
                .iter()
                .map(|action| format!("{}.{}", plugin.plugin_id, action.id))
        })
        .collect()
}
