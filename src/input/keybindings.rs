use crossterm::event::KeyCode;

use crate::config::{CustomCommandKeybind, KeyGroupAction, Keybinds};

use super::TerminalKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeybindDispatch {
    Direct,
    Prefix,
}

#[derive(Debug, Clone)]
pub(crate) enum KeybindMatch {
    Action(KeybindAction),
    Command(CustomCommandKeybind),
    /// Opens a which-key menu, built-in or user-defined, by id.
    Group(String),
    PluginAction(&'static str),
}

/// Resolves a key pressed while the `group` menu is open. User entries win
/// over built-in entries on the same key.
pub(crate) fn resolve_group_key(
    keybinds: &Keybinds,
    group: &str,
    key: &TerminalKey,
) -> Option<KeybindMatch> {
    resolve_group_key_exact(keybinds, group, key).or_else(|| {
        generated_character_key(key)
            .and_then(|generated_key| resolve_group_key_exact(keybinds, group, &generated_key))
    })
}

fn resolve_group_key_exact(
    keybinds: &Keybinds,
    group: &str,
    key: &TerminalKey,
) -> Option<KeybindMatch> {
    let member = keybinds
        .groups
        .iter()
        .find(|candidate| candidate.id == group)?
        .members
        .iter()
        .find(|member| member.keys.matches_direct_key(key))?;
    Some(match member.action {
        KeyGroupAction::Builtin(action) => KeybindMatch::Action(action),
        KeyGroupAction::PluginAction(action) => KeybindMatch::PluginAction(action),
        KeyGroupAction::Command(index) => {
            KeybindMatch::Command(keybinds.custom_commands.get(index)?.clone())
        }
    })
}

/// `(key label, description)` for every entry of the `group` menu: user
/// entries first, then built-in entries.
#[cfg(test)]
pub(crate) fn group_entries(
    keybinds: &Keybinds,
    group: &str,
) -> Vec<(String, std::borrow::Cow<'static, str>)> {
    group_entries_where(keybinds, group, |_| true)
}

/// [`group_entries`] limited to the members whose action `keep` accepts.
pub(crate) fn group_entries_where(
    keybinds: &Keybinds,
    group: &str,
    keep: impl Fn(&KeyGroupAction) -> bool,
) -> Vec<(String, std::borrow::Cow<'static, str>)> {
    keybinds
        .groups
        .iter()
        .filter(|candidate| candidate.id == group)
        .flat_map(|group| group.members.iter())
        .filter(|member| keep(&member.action))
        .map(|member| (member.label.clone(), member.description.clone()))
        .collect()
}

fn resolve_group_opener(keybinds: &Keybinds, key: &TerminalKey) -> Option<String> {
    keybinds
        .groups
        .iter()
        .find(|group| group.opener.matches_prefix_key(key))
        .map(|group| group.id.clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeybindAction {
    NewWorkspace,
    NewWorktree,
    OpenWorktree,
    RemoveWorktree,
    RenameWorkspace,
    CloseWorkspace,
    SwitchWorkspace(usize),
    SwitchTab(usize),
    FocusAgent(usize),
    WorkspacePicker,
    PreviousWorkspace,
    NextWorkspace,
    PreviousAgent,
    NextAgent,
    /// Agent-menu member only: agents are listed in a picker, so there is no
    /// flat `keys.*` override to configure.
    NewAgentTab,
    NewTab,
    RenameTab,
    PreviousTab,
    NextTab,
    MoveTabPrevious,
    MoveTabNext,
    MoveWorkspacePrevious,
    MoveWorkspaceNext,
    CloseTab,
    RenamePane,
    FocusPaneLeft,
    FocusPaneDown,
    FocusPaneUp,
    FocusPaneRight,
    SwapPaneLeft,
    SwapPaneDown,
    SwapPaneUp,
    SwapPaneRight,
    SplitVertical,
    SplitHorizontal,
    ClosePane,
    EditScrollback,
    ClearPane,
    CopyMode,
    Zoom,
    EnterResizeMode,
    ResizePaneLeft,
    ResizePaneDown,
    ResizePaneUp,
    ResizePaneRight,
    ToggleSidebar,
    ToggleGroup,
    ClearPaneName,
    SwapWithFocusedPane,
    CyclePaneNext,
    CyclePanePrevious,
    LastPane,
    Help,
    Settings,
    ReloadConfig,
    OpenNotificationTarget,
    Detach,
    OpenNavigator,
}

pub(crate) fn resolve_direct_binding(
    keybinds: &Keybinds,
    key: &TerminalKey,
) -> Option<KeybindMatch> {
    resolve_exact_binding(keybinds, key, KeybindDispatch::Direct)
}

pub(crate) fn resolve_prefix_binding(
    keybinds: &Keybinds,
    key: &TerminalKey,
) -> Option<KeybindMatch> {
    resolve_prefix_binding_exact(keybinds, key).or_else(|| {
        generated_character_key(key)
            .and_then(|generated_key| resolve_prefix_binding_exact(keybinds, &generated_key))
    })
}

fn resolve_prefix_binding_exact(keybinds: &Keybinds, key: &TerminalKey) -> Option<KeybindMatch> {
    resolve_group_opener(keybinds, key)
        .map(KeybindMatch::Group)
        .or_else(|| resolve_exact_binding(keybinds, key, KeybindDispatch::Prefix))
}

pub(crate) fn resolve_non_indexed_action(
    keybinds: &Keybinds,
    key: &TerminalKey,
    dispatch: KeybindDispatch,
) -> Option<KeybindAction> {
    flat_action_bindings(keybinds)
        .find(|(bindings, _)| action_matches(bindings, key, dispatch))
        .map(|(_, action)| action)
}

/// A built-in action with its own `keys.<name>` binding field.
struct FlatAction {
    /// The `keys.<name>` field, also the name a `[[keys.command]]` menu entry
    /// uses in `action = "<name>"`.
    name: &'static str,
    action: KeybindAction,
    bindings: fn(&Keybinds) -> &crate::config::ActionKeybinds,
}

macro_rules! flat {
    ($field:ident, $action:ident) => {
        FlatAction {
            name: stringify!($field),
            action: KeybindAction::$action,
            bindings: |keybinds| &keybinds.$field,
        }
    };
}

/// Every non-indexed action with its own `keys.<action>` binding, in help
/// and which-key order: grouped by the object acted on (pane, tab,
/// workspace, agent, git, session), hot-path actions first in each group.
const FLAT_ACTIONS: &[FlatAction] = &[
    flat!(focus_pane_left, FocusPaneLeft),
    flat!(focus_pane_down, FocusPaneDown),
    flat!(focus_pane_up, FocusPaneUp),
    flat!(focus_pane_right, FocusPaneRight),
    flat!(swap_pane_left, SwapPaneLeft),
    flat!(swap_pane_down, SwapPaneDown),
    flat!(swap_pane_up, SwapPaneUp),
    flat!(swap_pane_right, SwapPaneRight),
    flat!(cycle_pane_next, CyclePaneNext),
    flat!(cycle_pane_previous, CyclePanePrevious),
    flat!(last_pane, LastPane),
    flat!(swap_with_focused_pane, SwapWithFocusedPane),
    flat!(split_vertical, SplitVertical),
    flat!(split_horizontal, SplitHorizontal),
    flat!(zoom, Zoom),
    flat!(close_pane, ClosePane),
    flat!(resize_mode, EnterResizeMode),
    flat!(resize_pane_left, ResizePaneLeft),
    flat!(resize_pane_down, ResizePaneDown),
    flat!(resize_pane_up, ResizePaneUp),
    flat!(resize_pane_right, ResizePaneRight),
    flat!(copy_mode, CopyMode),
    flat!(rename_pane, RenamePane),
    flat!(clear_pane_name, ClearPaneName),
    flat!(edit_scrollback, EditScrollback),
    flat!(clear_pane, ClearPane),
    flat!(new_tab, NewTab),
    flat!(previous_tab, PreviousTab),
    flat!(next_tab, NextTab),
    flat!(move_tab_previous, MoveTabPrevious),
    flat!(move_tab_next, MoveTabNext),
    flat!(rename_tab, RenameTab),
    flat!(close_tab, CloseTab),
    flat!(new_workspace, NewWorkspace),
    flat!(rename_workspace, RenameWorkspace),
    flat!(close_workspace, CloseWorkspace),
    flat!(workspace_picker, WorkspacePicker),
    flat!(previous_workspace, PreviousWorkspace),
    flat!(next_workspace, NextWorkspace),
    flat!(move_workspace_previous, MoveWorkspacePrevious),
    flat!(move_workspace_next, MoveWorkspaceNext),
    flat!(toggle_group, ToggleGroup),
    flat!(previous_agent, PreviousAgent),
    flat!(next_agent, NextAgent),
    flat!(open_notification_target, OpenNotificationTarget),
    flat!(new_worktree, NewWorktree),
    flat!(open_worktree, OpenWorktree),
    flat!(remove_worktree, RemoveWorktree),
    flat!(goto, OpenNavigator),
    flat!(help, Help),
    flat!(settings, Settings),
    flat!(reload_config, ReloadConfig),
    flat!(toggle_sidebar, ToggleSidebar),
    flat!(detach, Detach),
];

/// Actions that exist only as menu entries: agents are listed in a picker,
/// so there is no flat `keys.*` field to configure.
const MENU_ONLY_ACTIONS: &[(&str, KeybindAction)] =
    &[("new_agent_tab", KeybindAction::NewAgentTab)];

/// Every non-indexed action with its own `keys.<action>` binding, in help
/// order. Shared by key resolution and the help projections.
pub(crate) fn flat_action_bindings(
    keybinds: &Keybinds,
) -> impl Iterator<Item = (&crate::config::ActionKeybinds, KeybindAction)> + '_ {
    FLAT_ACTIONS
        .iter()
        .map(move |flat| ((flat.bindings)(keybinds), flat.action))
}

impl KeybindAction {
    /// The built-in action a menu entry names with `action = "<name>"`.
    pub(crate) fn from_config_name(name: &str) -> Option<Self> {
        FLAT_ACTIONS
            .iter()
            .map(|flat| (flat.name, flat.action))
            .chain(MENU_ONLY_ACTIONS.iter().copied())
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, action)| action)
    }

    /// `(has a default binding, action)` for every flat action.
    #[cfg(test)]
    pub(crate) fn flat_defaults_for_test(keybinds: &Keybinds) -> Vec<(bool, KeybindAction)> {
        flat_action_bindings(keybinds)
            .map(|(bindings, action)| (!bindings.bindings.is_empty(), action))
            .collect()
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::NewWorkspace => "new workspace",
            Self::NewWorktree => "new worktree",
            Self::OpenWorktree => "open worktree",
            Self::RemoveWorktree => "remove worktree",
            Self::RenameWorkspace => "rename workspace",
            Self::CloseWorkspace => "close workspace",
            Self::SwitchWorkspace(_) => "switch workspace",
            Self::SwitchTab(_) => "switch tab",
            Self::FocusAgent(_) => "focus agent",
            Self::WorkspacePicker => "switch workspace",
            Self::PreviousWorkspace => "previous workspace",
            Self::NextWorkspace => "next workspace",
            Self::PreviousAgent => "previous agent",
            Self::NextAgent => "next agent",
            Self::NewAgentTab => "new agent tab",
            Self::NewTab => "new tab",
            Self::RenameTab => "rename tab",
            Self::PreviousTab => "previous tab",
            Self::NextTab => "next tab",
            Self::MoveTabPrevious => "move tab left",
            Self::MoveTabNext => "move tab right",
            Self::MoveWorkspacePrevious => "move workspace up",
            Self::MoveWorkspaceNext => "move workspace down",
            Self::CloseTab => "close tab",
            Self::RenamePane => "rename pane",
            Self::FocusPaneLeft => "focus pane left",
            Self::FocusPaneDown => "focus pane down",
            Self::FocusPaneUp => "focus pane up",
            Self::FocusPaneRight => "focus pane right",
            Self::SwapPaneLeft => "swap pane left",
            Self::SwapPaneDown => "swap pane down",
            Self::SwapPaneUp => "swap pane up",
            Self::SwapPaneRight => "swap pane right",
            Self::SplitVertical => "split side by side",
            Self::SplitHorizontal => "split stacked",
            Self::ClosePane => "close pane",
            Self::EditScrollback => "edit scrollback",
            Self::ClearPane => "clear screen and scrollback",
            Self::CopyMode => "copy mode",
            Self::Zoom => "zoom pane",
            Self::EnterResizeMode => "resize mode",
            Self::ResizePaneLeft => "resize pane left",
            Self::ResizePaneDown => "resize pane down",
            Self::ResizePaneUp => "resize pane up",
            Self::ResizePaneRight => "resize pane right",
            Self::ToggleSidebar => "toggle sidebar",
            Self::ToggleGroup => "expand/collapse group",
            Self::ClearPaneName => "clear pane name",
            Self::SwapWithFocusedPane => "swap with last pane",
            Self::CyclePaneNext => "cycle pane next",
            Self::CyclePanePrevious => "cycle pane previous",
            Self::LastPane => "last pane",
            Self::Help => "keybinds",
            Self::Settings => "settings",
            Self::ReloadConfig => "reload config",
            Self::OpenNotificationTarget => "jump to notification",
            Self::Detach => "detach",
            Self::OpenNavigator => "session navigator",
        }
    }
}

pub(crate) fn resolve_custom_command(
    keybinds: &Keybinds,
    key: &TerminalKey,
    dispatch: KeybindDispatch,
) -> Option<CustomCommandKeybind> {
    keybinds
        .custom_commands
        .iter()
        .filter(|binding| binding.group.is_none())
        .find(|binding| action_matches(&binding.bindings, key, dispatch))
        .cloned()
}

pub(crate) fn resolve_indexed_action(
    keybinds: &Keybinds,
    key: &TerminalKey,
    dispatch: KeybindDispatch,
) -> Option<KeybindAction> {
    let actual_modifiers = crate::config::normalize_key_combo((key.code, key.modifiers)).1;

    for exact_modifiers in [true, false] {
        let trigger_matches = |binding: &crate::config::IndexedKeybind| {
            let dispatch_matches = match dispatch {
                KeybindDispatch::Direct => binding.trigger.is_direct(),
                KeybindDispatch::Prefix => binding.trigger.is_prefix(),
            };
            let expected_modifiers = crate::config::normalize_key_combo(binding.trigger.combo()).1;
            dispatch_matches && (actual_modifiers == expected_modifiers) == exact_modifiers
        };

        for binding in &keybinds.switch_tab {
            if trigger_matches(binding) {
                if let Some(index) = binding.matched_index(key) {
                    return Some(KeybindAction::SwitchTab(index));
                }
            }
        }
        for binding in &keybinds.switch_workspace {
            if trigger_matches(binding) {
                if let Some(index) = binding.matched_index(key) {
                    return Some(KeybindAction::SwitchWorkspace(index));
                }
            }
        }
        for binding in &keybinds.focus_agent {
            if trigger_matches(binding) {
                if let Some(index) = binding.matched_index(key) {
                    return Some(KeybindAction::FocusAgent(index));
                }
            }
        }
    }

    None
}

fn resolve_exact_binding(
    keybinds: &Keybinds,
    key: &TerminalKey,
    dispatch: KeybindDispatch,
) -> Option<KeybindMatch> {
    resolve_non_indexed_action(keybinds, key, dispatch)
        .map(KeybindMatch::Action)
        .or_else(|| resolve_custom_command(keybinds, key, dispatch).map(KeybindMatch::Command))
        .or_else(|| resolve_indexed_action(keybinds, key, dispatch).map(KeybindMatch::Action))
}

fn generated_character_key(key: &TerminalKey) -> Option<TerminalKey> {
    let mut characters = key.generated_text.as_deref()?.chars();
    let character = characters.next()?;
    if character.is_control() || characters.next().is_some() {
        return None;
    }
    Some(TerminalKey::new(
        KeyCode::Char(character),
        crossterm::event::KeyModifiers::empty(),
    ))
}

fn action_matches(
    bindings: &crate::config::ActionKeybinds,
    key: &TerminalKey,
    dispatch: KeybindDispatch,
) -> bool {
    match dispatch {
        KeybindDispatch::Direct => bindings.matches_direct_key(key),
        KeybindDispatch::Prefix => bindings.matches_prefix_key(key),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;

    fn key(code: KeyCode) -> TerminalKey {
        TerminalKey::new(code, KeyModifiers::empty())
    }

    #[test]
    fn clear_pane_is_unbound_by_default_and_configurable() {
        assert!(crate::config::Config::default()
            .keybinds()
            .clear_pane
            .bindings
            .is_empty());
        let config: crate::config::Config =
            toml::from_str("[keys]\nclear_pane = [\"super+k\", \"prefix+ctrl+k\"]").unwrap();
        assert!(config.collect_diagnostics().is_empty());
        let keybinds = config.keybinds();
        assert!(matches!(
            resolve_direct_binding(
                &keybinds,
                &TerminalKey::new(KeyCode::Char('k'), KeyModifiers::SUPER)
            ),
            Some(KeybindMatch::Action(KeybindAction::ClearPane))
        ));
        assert!(matches!(
            resolve_prefix_binding(
                &keybinds,
                &TerminalKey::new(KeyCode::Char('k'), KeyModifiers::CONTROL)
            ),
            Some(KeybindMatch::Action(KeybindAction::ClearPane))
        ));
        assert!(matches!(
            resolve_prefix_binding(
                &keybinds,
                &TerminalKey::new(KeyCode::Char('k'), KeyModifiers::SHIFT)
            ),
            Some(KeybindMatch::Action(KeybindAction::SwapPaneUp))
        ));
    }

    #[test]
    fn flat_action_names_are_unique_keys_fields() {
        let keys = toml::Value::try_from(&Config::default().keys).unwrap();
        let mut seen = std::collections::HashSet::new();
        for flat in FLAT_ACTIONS {
            assert!(seen.insert(flat.name), "duplicate {}", flat.name);
            assert!(keys.get(flat.name).is_some(), "keys.{} missing", flat.name);
            assert_eq!(
                KeybindAction::from_config_name(flat.name),
                Some(flat.action)
            );
        }
        for (name, _) in MENU_ONLY_ACTIONS {
            assert!(seen.insert(name) && keys.get(name).is_none(), "{name}");
        }
    }

    #[test]
    fn one_shared_resolver_handles_direct_prefix_and_indexed_bindings() {
        let keybinds = Keybinds {
            next_tab: crate::config::ActionKeybinds::direct("ctrl+n"),
            ..Keybinds::default()
        };

        let direct = TerminalKey::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        assert!(matches!(
            resolve_direct_binding(&keybinds, &direct),
            Some(KeybindMatch::Action(KeybindAction::NextTab))
        ));

        assert!(matches!(
            resolve_prefix_binding(&keybinds, &key(KeyCode::Char('?'))),
            Some(KeybindMatch::Action(KeybindAction::Help))
        ));

        assert!(matches!(
            resolve_prefix_binding(&keybinds, &key(KeyCode::Char('1'))),
            Some(KeybindMatch::Action(KeybindAction::SwitchTab(0)))
        ));
    }

    #[test]
    fn prefix_resolution_uses_shared_generated_character_fallback() {
        let keybinds = Keybinds::default();
        let key = TerminalKey::new(KeyCode::Char('/'), KeyModifiers::SHIFT)
            .with_generated_text(Some("?".to_owned()));

        assert!(matches!(
            resolve_prefix_binding(&keybinds, &key),
            Some(KeybindMatch::Action(KeybindAction::Help))
        ));
    }

    #[test]
    fn group_keys_use_the_generated_character_fallback() {
        let keybinds = Keybinds::default();
        let key = TerminalKey::new(KeyCode::Char('/'), KeyModifiers::SHIFT)
            .with_generated_text(Some("?".to_owned()));

        assert!(matches!(
            resolve_group_key(&keybinds, "system", &key),
            Some(KeybindMatch::Action(KeybindAction::Help))
        ));
    }

    #[test]
    fn default_menu_openers_resolve_to_groups() {
        let keybinds = Keybinds::default();
        for (ch, group) in [
            ('w', "workspace"),
            ('t', "tab"),
            ('p', "pane"),
            ('a', "agent"),
            ('g', "git"),
            ('s', "system"),
        ] {
            match resolve_prefix_binding(&keybinds, &key(KeyCode::Char(ch))) {
                Some(KeybindMatch::Group(id)) => assert_eq!(id, group),
                other => panic!("prefix+{ch}: expected {group} menu, got {other:?}"),
            }
        }
    }

    #[test]
    fn menus_share_one_verb_vocabulary() {
        let keybinds = Keybinds::default();
        for (group, ch, action) in [
            ("workspace", 'n', KeybindAction::NewWorkspace),
            ("tab", 'n', KeybindAction::NewTab),
            ("git", 'n', KeybindAction::NewWorktree),
            ("workspace", 'r', KeybindAction::RenameWorkspace),
            ("tab", 'r', KeybindAction::RenameTab),
            ("pane", 'r', KeybindAction::RenamePane),
            ("workspace", 'x', KeybindAction::CloseWorkspace),
            ("tab", 'x', KeybindAction::CloseTab),
            ("pane", 'x', KeybindAction::ClosePane),
            ("git", 'x', KeybindAction::RemoveWorktree),
        ] {
            match resolve_group_key(&keybinds, group, &key(KeyCode::Char(ch))) {
                Some(KeybindMatch::Action(found)) => assert_eq!(found, action, "{group} {ch}"),
                other => panic!("{group} {ch}: expected {action:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn user_bound_flat_actions_resolve_at_top_level() {
        let config: Config = toml::from_str("[keys]\nnew_tab = \"prefix+c\"\n").unwrap();
        assert!(matches!(
            resolve_prefix_binding(&config.keybinds(), &key(KeyCode::Char('c'))),
            Some(KeybindMatch::Action(KeybindAction::NewTab))
        ));
    }

    #[test]
    fn user_custom_command_displaces_a_default_menu_opener_with_a_diagnostic() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "prefix+s"
type = "plugin_action"
command = "worktrunk.list"
description = "open lazygit"
"#,
        )
        .unwrap();
        let keybinds = config.keybinds();
        match resolve_prefix_binding(&keybinds, &key(KeyCode::Char('s'))) {
            Some(KeybindMatch::Command(command)) => {
                assert_eq!(command.command, "worktrunk.list");
            }
            other => panic!("expected custom command, got {other:?}"),
        }
        assert!(config
            .collect_diagnostics()
            .iter()
            .any(|diag| diag.contains("keys.system_menu")));
        // The menu still exists and its opener can be moved.
        assert!(keybinds.groups.iter().any(|group| group.id == "system"));
    }

    #[test]
    fn user_entry_joins_and_overrides_a_builtin_menu() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "n"
group = "tab"
command = "echo custom"
description = "custom new tab"
"#,
        )
        .unwrap();
        let keybinds = config.keybinds();
        match resolve_group_key(&keybinds, "tab", &key(KeyCode::Char('n'))) {
            Some(KeybindMatch::Command(command)) => assert_eq!(command.command, "echo custom"),
            other => panic!("expected user entry, got {other:?}"),
        }
        let entries = group_entries(&keybinds, "tab");
        assert_eq!(entries.iter().filter(|(key, _)| key == "n").count(), 1);
    }
}
